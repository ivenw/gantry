use crate::session::{Node, NodeId};
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
    pub node: Node,
    pub branches: Vec<Branch>,
}

/// Builds a `Branch` tree from `nodes`, rooted at the single node with no parent.
///
/// Returns `None` if `nodes` is empty. When a node has multiple children, each child
/// becomes a separate entry in `branches` and traversal of that path stops — the caller
/// recurses into sub-branches via `branches`.
pub fn build_branch(nodes: &HashMap<NodeId, Node>) -> Option<Branch> {
    let root_id = nodes.values().find(|n| n.parent_id.is_none())?.id.clone();
    build_from(nodes, root_id)
}

fn build_from(nodes: &HashMap<NodeId, Node>, root_id: NodeId) -> Option<Branch> {
    let root = nodes.get(&root_id)?.clone();

    let mut children: Vec<&Node> = nodes
        .values()
        .filter(|n| n.parent_id.as_ref() == Some(&root_id))
        .collect();
    children.sort_by_key(|n| &n.id);

    let branches = match children.len() {
        0 => vec![],
        1 => build_from(nodes, children[0].id.clone())
            .map(|child| vec![child])
            .unwrap_or_default(),
        _ => children
            .iter()
            .filter_map(|c| build_from(nodes, c.id.clone()))
            .collect(),
    };

    Some(Branch {
        node: root,
        branches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirs::{ProjectConfigDir, ProjectRootDir};
    use crate::fs::FsSessionRegistry;
    use crate::message::Message;
    use crate::session::registry::SessionRegistry;
    use crate::session::{Session, SessionHistory};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn registry() -> (TempDir, FsSessionRegistry) {
        let tmp = TempDir::new().unwrap();
        let root = ProjectRootDir::new(tmp.path()).unwrap();
        let config_dir = ProjectConfigDir::new(&root).unwrap();
        let r = FsSessionRegistry::new(&config_dir).unwrap();
        (tmp, r)
    }

    fn nodes_from_session(session: &Session<impl SessionHistory>) -> HashMap<NodeId, Node> {
        session
            .all_nodes()
            .map(|n| (n.id.clone(), n.clone()))
            .collect()
    }

    fn build_tree_for_test(session: &Session<impl SessionHistory>) -> Option<Branch> {
        let nodes = nodes_from_session(session);
        build_branch(&nodes)
    }

    #[test]
    fn build_branch_empty() {
        let nodes = HashMap::new();
        let branch = build_branch(&nodes);
        assert!(branch.is_none());
    }

    #[test]
    fn build_branch_linear() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();
        session.append_message(Message::user("root")).unwrap();
        session.append_message(Message::assistant("mid")).unwrap();
        session.append_message(Message::user("leaf")).unwrap();

        let branch = build_tree_for_test(&session).unwrap();

        // root -> mid -> leaf: each node has exactly one child until the leaf
        assert_eq!(branch.branches.len(), 1);
        assert_eq!(branch.branches[0].branches.len(), 1);
        assert!(branch.branches[0].branches[0].branches.is_empty());
    }

    #[test]
    fn build_branch_two_children() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();
        session.append_message(Message::user("root")).unwrap();
        let root_id = session.current_leaf_id.clone().unwrap();
        session
            .append_message(Message::assistant("child A"))
            .unwrap();
        session.branch(&root_id).unwrap();
        session
            .append_message(Message::assistant("child B"))
            .unwrap();

        let branch = build_tree_for_test(&session).unwrap();

        assert_eq!(branch.branches.len(), 2);
        assert!(branch.branches[0].branches.is_empty());
        assert!(branch.branches[1].branches.is_empty());
    }

    #[test]
    fn build_branch_linear_then_fork() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();
        session.append_message(Message::user("root")).unwrap();
        session.append_message(Message::assistant("mid")).unwrap();
        let mid_id = session.current_leaf_id.clone().unwrap();
        session.append_message(Message::user("child A")).unwrap();
        session.branch(&mid_id).unwrap();
        session.append_message(Message::user("child B")).unwrap();

        let branch = build_tree_for_test(&session).unwrap();

        // root has one child (mid), mid has two children (child A, child B)
        assert_eq!(branch.branches.len(), 1);
        assert_eq!(branch.branches[0].branches.len(), 2);
    }

    #[test]
    fn build_branch_deep_nest() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();
        session.append_message(Message::user("root")).unwrap();
        let root_id = session.current_leaf_id.clone().unwrap();
        session.append_message(Message::assistant("A")).unwrap();
        let a_id = session.current_leaf_id.clone().unwrap();
        session.append_message(Message::user("B")).unwrap();
        session.branch(&a_id).unwrap();
        session.append_message(Message::user("C")).unwrap();
        session.branch(&root_id).unwrap();
        session.append_message(Message::assistant("D")).unwrap();

        let branch = build_tree_for_test(&session).unwrap();

        assert_eq!(branch.branches.len(), 2);

        let sub_with_a = branch
            .branches
            .iter()
            .find(|b| {
                matches!(&b.node.message, Message::Assistant { content, .. } if {
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
                matches!(&b.node.message, Message::Assistant { content, .. } if {
                    use rig::message::AssistantContent;
                    content.iter().any(|c| matches!(c, AssistantContent::Text(t) if t.text == "D"))
                })
            })
            .unwrap();

        assert!(sub_with_d.branches.is_empty());
    }
}
