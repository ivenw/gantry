pub mod registry;
pub mod tree;

pub use registry::{SessionInfo, SessionRegistry};
pub use tree::{Branch, SessionTree};

use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::Message;
use crate::metrics::Usage;

use crate::session::tree::build_branch;

/// A unique identifier for a session node.
pub type NodeId = Id<NodeTag>;
/// A unique identifier for a session.
pub type SessionId = Id<SessionTag>;

/// Marker type for [`Id`] scoped to a session node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeTag;
/// Marker type for [`Id`] scoped to a session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionTag;

/// A typed UUID wrapper used to distinguish session-scoped identifiers at the type level.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id<T> {
    value: Uuid,
    #[serde(skip)]
    _marker: PhantomData<T>,
}

impl<T> Id<T> {
    /// Generates a new random ID.
    pub fn new() -> Self {
        Self {
            value: Uuid::new_v4(),
            _marker: PhantomData,
        }
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T> FromStr for Id<T> {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self {
            value: Uuid::parse_str(s)?,
            _marker: PhantomData,
        })
    }
}

/// An in-memory representation of a single conversation session.
pub struct Session<H: SessionHistory> {
    pub session_id: SessionId,
    pub current_leaf_id: Option<NodeId>,
    nodes: HashMap<NodeId, Node>,
    history: H,
}

/// Abstracts the persistence of session nodes.
pub trait SessionHistory {
    /// Appends a new node to the history.
    fn append(&self, node: &Node) -> Result<()>;

    /// Returns all nodes in the order they were appended.
    fn nodes(&self) -> Result<Vec<Node>>;
}

impl<H: SessionHistory> Session<H> {
    /// Creates a new empty session backed by the given history.
    pub(super) fn new(session_id: SessionId, history: H) -> Self {
        Self {
            session_id,
            current_leaf_id: None,
            nodes: HashMap::new(),
            history,
        }
    }

    /// Restores a session from its persisted history, setting the active leaf to the most
    /// recently created tip node.
    pub(super) fn restore(session_id: SessionId, history: H) -> Result<Self> {
        let nodes_vec = history
            .nodes()
            .with_context(|| format!("failed to load session {}", session_id))?;

        let nodes: HashMap<NodeId, Node> =
            nodes_vec.into_iter().map(|n| (n.id.clone(), n)).collect();

        let parent_ids: std::collections::HashSet<&NodeId> = nodes
            .values()
            .filter_map(|n| n.parent_id.as_ref())
            .collect();

        let current_leaf_id = nodes
            .values()
            .filter(|n| !parent_ids.contains(&n.id))
            .max_by_key(|n| n.timestamp)
            .map(|n| n.id.clone());

        Ok(Self {
            session_id,
            current_leaf_id,
            nodes,
            history,
        })
    }

    /// Appends a [`Message`] to the session as a new node, persisting it to history.
    pub fn append_message(&mut self, message: Message) -> Result<()> {
        self.append_message_with_usage(message, None)
    }

    /// Appends a [`Message`] with optional token usage, persisting it to history.
    ///
    /// `usage` should be `Some` for assistant messages produced by a model response.
    pub fn append_message_with_usage(
        &mut self,
        message: Message,
        usage: Option<Usage>,
    ) -> Result<()> {
        let node = Node::new(message, self.current_leaf_id.clone(), usage);
        self.history
            .append(&node)
            .with_context(|| format!("failed to persist node to session {}", self.session_id))?;
        let id = node.id.clone();
        self.current_leaf_id = Some(id.clone());
        self.nodes.insert(id, node);
        Ok(())
    }

    /// Switches the active leaf to `from_node_id`, allowing messages to be appended on a new
    /// branch from that point.
    pub fn branch(&mut self, from_node_id: &NodeId) -> Result<()> {
        if !self.nodes.contains_key(from_node_id) {
            return Err(anyhow::anyhow!(
                "node {} not found in session {}",
                from_node_id,
                self.session_id
            ));
        }
        self.current_leaf_id = Some(from_node_id.clone());
        Ok(())
    }

    /// Returns the ordered [`Message`]s on the active branch from root to the current leaf.
    pub fn history(&self) -> Vec<Message> {
        let Some(leaf_id) = &self.current_leaf_id else {
            return vec![];
        };

        let mut chain: Vec<&Node> = vec![];
        let mut current_id = leaf_id;
        loop {
            let Some(node) = self.nodes.get(current_id) else {
                break;
            };
            chain.push(node);
            match node.parent_id.as_ref() {
                Some(parent_id) => current_id = parent_id,
                None => break,
            }
        }

        chain.reverse();
        chain.into_iter().map(|n| n.message.clone()).collect()
    }

    /// Returns an iterator over all nodes in the session, in arbitrary order.
    pub fn all_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Sums token consumption across all nodes in the session, regardless of branch.
    pub fn total_consumption(&self) -> Usage {
        self.nodes
            .values()
            .filter_map(|n| n.usage.clone())
            .fold(Usage::default(), |mut acc, c| {
                acc += c;
                acc
            })
    }

    /// Builds and returns the session tree for UI rendering.
    ///
    /// Returns `None` if the session has no nodes.
    pub fn as_tree(&self) -> Option<SessionTree> {
        let current_leaf_id = self.current_leaf_id.clone()?;
        let nodes: HashMap<NodeId, Node> = self
            .all_nodes()
            .map(|n| (n.id.clone(), n.clone()))
            .collect();
        let stem = build_branch(&nodes)?;
        Some(SessionTree {
            current_leaf_id,
            stem,
        })
    }
}

/// A tree node in the session history, wrapping a [`Message`] with parent linkage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub timestamp: Timestamp,
    pub message: Message,
    /// Token usage for the request that produced this node, set on assistant nodes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl Node {
    /// Creates a new node with a fresh ID, the current timestamp, and the given message.
    pub fn new(message: Message, parent_id: Option<NodeId>, usage: Option<Usage>) -> Self {
        Self {
            id: NodeId::new(),
            parent_id,
            timestamp: Timestamp::now(),
            message,
            usage,
        }
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
    fn create_returns_empty_session() {
        let (_tmp, r) = registry();
        let session = r.create_session().unwrap();
        assert!(session.current_leaf_id.is_none());
        assert_eq!(session.history().len(), 0);
    }

    #[test]
    fn append_advances_leaf_and_persists() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();

        session.append_message(Message::user("hello")).unwrap();
        assert!(session.current_leaf_id.is_some());

        session.append_message(Message::assistant("hi")).unwrap();

        let ctx = session.history();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0], Message::user("hello"));
        assert_eq!(ctx[1], Message::assistant("hi"));
    }

    #[test]
    fn append_sets_parent_id() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();

        session.append_message(Message::user("root")).unwrap();
        let first_id = session.current_leaf_id.clone().unwrap();
        session.append_message(Message::assistant("reply")).unwrap();

        let leaf_id = session.current_leaf_id.clone().unwrap();
        assert_eq!(session.nodes[&leaf_id].parent_id.as_ref(), Some(&first_id));
    }

    #[test]
    fn branch_switches_active_leaf() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();

        session.append_message(Message::user("root")).unwrap();
        let root_id = session.current_leaf_id.clone().unwrap();
        session
            .append_message(Message::assistant("branch A"))
            .unwrap();

        session.branch(&root_id).unwrap();
        session.append_message(Message::user("branch B")).unwrap();

        let ctx = session.history();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0], Message::user("root"));
        assert_eq!(ctx[1], Message::user("branch B"));
    }

    #[test]
    fn branch_errors_on_unknown_id() {
        let (_tmp, r) = registry();
        let mut session = r.create_session().unwrap();
        let fake_id: NodeId = "00000000-0000-0000-0000-000000000000".parse().unwrap();
        assert!(session.branch(&fake_id).is_err());
    }

    #[test]
    fn load_restores_session_and_picks_latest_leaf() {
        let (_tmp, r) = registry();
        let session_id = {
            let mut session = r.create_session().unwrap();
            session.append_message(Message::user("first")).unwrap();
            session
                .append_message(Message::assistant("second"))
                .unwrap();
            session.session_id.clone()
        };

        let session = r.load_session(&session_id).unwrap();
        let ctx = session.history();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0], Message::user("first"));
        assert_eq!(ctx[1], Message::assistant("second"));
    }

    #[test]
    fn list_returns_created_sessions() {
        let (_tmp, r) = registry();
        let id = r.create_session().unwrap().session_id;
        let sessions = r.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
    }

    #[test]
    fn tool_result_reconstructed_as_pair() {
        let (_tmp, r) = registry();
        let session_id = {
            let mut session = r.create_session().unwrap();
            session.append_message(Message::user("run it")).unwrap();
            session
                .append_message(Message::tool_result("call-1", "done"))
                .unwrap();
            session.session_id.clone()
        };

        let session = r.load_session(&session_id).unwrap();
        let ctx = session.history();
        assert_eq!(ctx.len(), 2);
        assert!(matches!(ctx[0], Message::User { .. }));
        assert!(matches!(ctx[1], Message::User { .. }));
    }
}
