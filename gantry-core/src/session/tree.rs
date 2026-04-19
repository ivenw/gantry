use crate::session::store::SessionEntry;
use crate::chat::Role;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The full message tree for a session, including which node is the active conversation tip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTree {
    pub current_leaf_id: Option<String>,
    pub stem: Branch,
}

/// An ordered sequence of nodes at a given nesting depth, with sub-branches forking off nodes
/// that have multiple children.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub depth: usize,
    pub nodes: Vec<BranchNode>,
}

/// A single message node in the tree, carrying any sub-branches that fork from it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchNode {
    pub id: String,
    pub role: Role,
    pub content: String,
    pub branches: Vec<Branch>,
}

/// Builds a `Branch` from `start_id` forward, forking into sub-branches when a node has multiple
/// children.
///
/// `depth` is the nesting level of this branch and is stored on the returned `Branch`.
pub fn build_branch(
    entries: &HashMap<String, SessionEntry>,
    start_id: Option<String>,
    depth: usize,
) -> Branch {
    let mut nodes = Vec::new();
    let mut cursor = start_id;
    while let Some(ref id) = cursor {
        let node_id = id.clone();
        let Some(SessionEntry::Message(m)) = entries.get(&node_id) else {
            break;
        };
        let mut children: Vec<&SessionEntry> = entries
            .values()
            .filter(|e| e.parent_id() == Some(node_id.as_str()))
            .collect();
        children.sort_by_key(|e| e.id());
        match children.len() {
            0 => {
                nodes.push(BranchNode {
                    id: m.base.id.clone(),
                    role: m.role,
                    content: m.content.clone(),
                    branches: vec![],
                });
                break;
            }
            1 => {
                let next_id = children[0].id().to_string();
                nodes.push(BranchNode {
                    id: m.base.id.clone(),
                    role: m.role,
                    content: m.content.clone(),
                    branches: vec![],
                });
                cursor = Some(next_id);
            }
            _ => {
                let sub_branches = children
                    .iter()
                    .map(|c| build_branch(entries, Some(c.id().to_string()), depth + 1))
                    .collect();
                nodes.push(BranchNode {
                    id: m.base.id.clone(),
                    role: m.role,
                    content: m.content.clone(),
                    branches: sub_branches,
                });
                break;
            }
        }
    }
    Branch { depth, nodes }
}
