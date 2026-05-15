use crate::message::Message;
use crate::metrics::Usage;
use crate::session::{NodeId, Session, SessionHistory};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A snapshot of the session history tree paired with the currently active leaf.
///
/// Both fields are derived from the same session state, so they are always consistent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTree {
    pub current_leaf_id: NodeId,
    pub stem: Branch,
}

/// A node in the session tree view, carrying the full node data and any child branches.
///
/// This is a projection of session history optimised for UI rendering. The root
/// `Branch` is the single stem; forks appear as multiple entries in `branches`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub timestamp: Timestamp,
    pub message: Message,
    pub usage: Option<Usage>,
    pub branches: Vec<Branch>,
}

/// Builds a `Branch` tree from the session, rooted at the session's root node.
pub fn build_branch<H: SessionHistory>(session: &Session<H>) -> Branch {
    // Collect children indexed by parent for fast lookup.
    let mut children_by_parent: HashMap<&NodeId, Vec<&NodeId>> = HashMap::new();
    for node in session.all_nodes() {
        if let Some(pid) = node.parent_id {
            children_by_parent.entry(pid).or_default().push(node.id);
        }
    }

    build_from(session, &session.root.id, &children_by_parent)
}

fn build_from<H: SessionHistory>(
    session: &Session<H>,
    node_id: &NodeId,
    children_by_parent: &HashMap<&NodeId, Vec<&NodeId>>,
) -> Branch {
    let flat = session
        .all_nodes()
        .find(|n| n.id == node_id)
        .expect("node_id must exist in session");

    let mut child_ids: Vec<&&NodeId> = children_by_parent
        .get(node_id)
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    child_ids.sort();

    let branches = match child_ids.len() {
        0 => vec![],
        1 => vec![build_from(session, child_ids[0], children_by_parent)],
        _ => child_ids
            .iter()
            .map(|id| build_from(session, id, children_by_parent))
            .collect(),
    };

    Branch {
        id: flat.id.clone(),
        parent_id: flat.parent_id.cloned(),
        timestamp: flat.timestamp,
        message: flat.message.clone(),
        usage: flat.usage.cloned(),
        branches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::FsSessionRegistry;
    use crate::message::Message;
    use crate::session::registry::SessionRegistry;
    use tempfile::TempDir;

    fn registry() -> (TempDir, FsSessionRegistry) {
        let tmp = TempDir::new().unwrap();
        let r = FsSessionRegistry::new(tmp.path()).unwrap();
        (tmp, r)
    }

    #[test]
    fn build_branch_single_root() {
        let (_tmp, r) = registry();
        let session = r.create_session(Message::user("root")).unwrap();
        let branch = build_branch(&session);
        assert_eq!(branch.message, Message::user("root"));
        assert!(branch.branches.is_empty());
    }

    #[test]
    fn build_branch_linear() {
        let (_tmp, r) = registry();
        let mut session = r.create_session(Message::user("root")).unwrap();
        session.append_message(Message::assistant("mid")).unwrap();
        session.append_message(Message::user("leaf")).unwrap();

        let branch = build_branch(&session);

        // root -> mid -> leaf: each node has exactly one child until the leaf
        assert_eq!(branch.branches.len(), 1);
        assert_eq!(branch.branches[0].branches.len(), 1);
        assert!(branch.branches[0].branches[0].branches.is_empty());
    }

    #[test]
    fn build_branch_two_children() {
        let (_tmp, r) = registry();
        let mut session = r.create_session(Message::user("root")).unwrap();
        let root_id = session.current_leaf_id.clone();
        session
            .append_message(Message::assistant("child A"))
            .unwrap();
        session.branch(&root_id).unwrap();
        session
            .append_message(Message::assistant("child B"))
            .unwrap();

        let branch = build_branch(&session);

        assert_eq!(branch.branches.len(), 2);
        assert!(branch.branches[0].branches.is_empty());
        assert!(branch.branches[1].branches.is_empty());
    }

    #[test]
    fn build_branch_linear_then_fork() {
        let (_tmp, r) = registry();
        let mut session = r.create_session(Message::user("root")).unwrap();
        session.append_message(Message::assistant("mid")).unwrap();
        let mid_id = session.current_leaf_id.clone();
        session.append_message(Message::user("child A")).unwrap();
        session.branch(&mid_id).unwrap();
        session.append_message(Message::user("child B")).unwrap();

        let branch = build_branch(&session);

        // root has one child (mid), mid has two children (child A, child B)
        assert_eq!(branch.branches.len(), 1);
        assert_eq!(branch.branches[0].branches.len(), 2);
    }

    #[test]
    fn build_branch_deep_nest() {
        let (_tmp, r) = registry();
        let mut session = r.create_session(Message::user("root")).unwrap();
        let root_id = session.current_leaf_id.clone();
        session.append_message(Message::assistant("A")).unwrap();
        let a_id = session.current_leaf_id.clone();
        session.append_message(Message::user("B")).unwrap();
        session.branch(&a_id).unwrap();
        session.append_message(Message::user("C")).unwrap();
        session.branch(&root_id).unwrap();
        session.append_message(Message::assistant("D")).unwrap();

        let branch = build_branch(&session);

        assert_eq!(branch.branches.len(), 2);

        let sub_with_a = branch
            .branches
            .iter()
            .find(|b| {
                matches!(&b.message, Message::Assistant { content, .. } if {
                    use rig::message::AssistantContent;
                    content.iter().any(|c| matches!(c, AssistantContent::Text(t) if t.text == "A"))
                })
            })
            .unwrap();
        assert_eq!(sub_with_a.branches.len(), 2);

        let sub_with_d = branch
            .branches
            .iter()
            .find(|b| {
                matches!(&b.message, Message::Assistant { content, .. } if {
                    use rig::message::AssistantContent;
                    content.iter().any(|c| matches!(c, AssistantContent::Text(t) if t.text == "D"))
                })
            })
            .unwrap();

        assert!(sub_with_d.branches.is_empty());
    }
}
