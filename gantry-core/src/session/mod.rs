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

/// A tagged union of [`RootNode`] and [`ChildNode`] used for persistence.
///
/// The `type` tag distinguishes the two variants when serialized.
#[derive(Debug, Clone)]
pub enum StoredNode {
    Root(RootNode),
    Child(ChildNode),
}

/// The single root node of a session, carrying its first message.
///
/// A session always has exactly one `RootNode`. Having no `parent_id` field makes it
/// structurally impossible to create a second root.
#[derive(Debug, Clone)]
pub struct RootNode {
    pub id: NodeId,
    pub timestamp: Timestamp,
    pub message: Message,
}

impl RootNode {
    /// Creates a new root node with a fresh ID and the current timestamp.
    pub fn new(message: Message) -> Self {
        Self {
            id: NodeId::new(),
            timestamp: Timestamp::now(),
            message,
        }
    }
}

/// A non-root node in the session tree, always linked to a parent.
///
/// The mandatory `parent_id` field makes it structurally impossible for a `ChildNode`
/// to be a root.
#[derive(Debug, Clone)]
pub struct ChildNode {
    pub id: NodeId,
    pub parent_id: NodeId,
    pub timestamp: Timestamp,
    pub message: Message,
    /// Token usage for the request that produced this node, set on assistant nodes only.
    pub usage: Option<Usage>,
}

impl ChildNode {
    /// Creates a new child node with a fresh ID and the current timestamp.
    pub fn new(message: Message, parent_id: NodeId, usage: Option<Usage>) -> Self {
        Self {
            id: NodeId::new(),
            parent_id,
            timestamp: Timestamp::now(),
            message,
            usage,
        }
    }
}

/// An in-memory representation of a single conversation session.
pub struct Session<H: SessionHistory> {
    pub session_id: SessionId,
    pub current_leaf_id: NodeId,
    pub root: RootNode,
    children: HashMap<NodeId, ChildNode>,
    history: H,
}

/// Abstracts the persistence of session nodes.
pub trait SessionHistory {
    /// Appends a stored node to the history.
    fn append(&self, node: &StoredNode) -> Result<()>;

    /// Returns all stored nodes in the order they were appended.
    fn nodes(&self) -> Result<Vec<StoredNode>>;
}

// TODO: There are many methods that are independent of the history, i wonder if they need to
// live under the generic H impl.
impl<H: SessionHistory> Session<H> {
    /// Creates a new session with the given root message.
    pub(super) fn new(session_id: SessionId, root: RootNode, history: H) -> Self {
        let current_leaf_id = root.id.clone();
        Self {
            session_id,
            current_leaf_id,
            root,
            children: HashMap::new(),
            history,
        }
    }

    /// Restores a session from its persisted history, setting the active leaf to the most
    /// recently created tip node.
    ///
    /// Panics if the history has no root node or more than one root node.
    pub(super) fn restore(session_id: SessionId, history: H) -> Result<Self> {
        let stored = history
            .nodes()
            .with_context(|| format!("failed to load session {}", session_id))?;

        let mut root: Option<RootNode> = None;
        let mut children: HashMap<NodeId, ChildNode> = HashMap::new();

        for node in stored {
            match node {
                StoredNode::Root(r) => {
                    assert!(
                        root.is_none(),
                        "session {} has multiple root nodes; a session must have exactly one root",
                        session_id
                    );
                    root = Some(r);
                }
                StoredNode::Child(c) => {
                    children.insert(c.id.clone(), c);
                }
            }
        }

        let root = root.unwrap_or_else(|| {
            panic!(
                "session {} has no root node; a session must have exactly one root",
                session_id
            )
        });

        let child_parent_ids: std::collections::HashSet<&NodeId> =
            children.values().map(|c| &c.parent_id).collect();

        // The current leaf is the tip with the latest timestamp that is not a parent of
        // any other child.
        let current_leaf_id = {
            let root_is_tip = !child_parent_ids.contains(&root.id);
            let child_tips = children
                .values()
                .filter(|c| !child_parent_ids.contains(&c.id));

            if root_is_tip {
                std::iter::once((root.timestamp, root.id.clone()))
                    .chain(child_tips.map(|c| (c.timestamp, c.id.clone())))
                    .max_by_key(|(ts, _)| *ts)
                    .map(|(_, id)| id)
                    .unwrap()
            } else {
                child_tips
                    .max_by_key(|c| c.timestamp)
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| root.id.clone())
            }
        };

        Ok(Self {
            session_id,
            current_leaf_id,
            root,
            children,
            history,
        })
    }

    /// Appends a [`Message`] to the session as a new child node, persisting it to history.
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
        let child = ChildNode::new(message, self.current_leaf_id.clone(), usage);
        self.history
            .append(&StoredNode::Child(child.clone()))
            .with_context(|| format!("failed to persist node to session {}", self.session_id))?;
        self.current_leaf_id = child.id.clone();
        self.children.insert(child.id.clone(), child);
        Ok(())
    }

    /// Switches the active leaf to `from_node_id`, allowing messages to be appended on a new
    /// branch from that point.
    pub fn branch(&mut self, from_node_id: &NodeId) -> Result<()> {
        if *from_node_id != self.root.id && !self.children.contains_key(from_node_id) {
            return Err(anyhow::anyhow!(
                "node {} not found in session {}",
                from_node_id,
                self.session_id
            ));
        }
        self.current_leaf_id = from_node_id.clone();
        Ok(())
    }

    /// Returns the ordered [`Message`]s on the active branch from root to the current leaf.
    pub fn history(&self) -> Vec<Message> {
        let mut chain: Vec<&Message> = vec![];
        let mut current_id = &self.current_leaf_id;
        loop {
            if *current_id == self.root.id {
                chain.push(&self.root.message);
                break;
            }
            let Some(child) = self.children.get(current_id) else {
                break;
            };
            chain.push(&child.message);
            current_id = &child.parent_id;
        }
        chain.reverse();
        chain.into_iter().cloned().collect()
    }

    /// Returns an iterator over all nodes as a unified flat view, root first.
    pub fn all_nodes(&self) -> impl Iterator<Item = FlatNode<'_>> {
        let root = FlatNode {
            id: &self.root.id,
            parent_id: None,
            timestamp: self.root.timestamp,
            message: &self.root.message,
            usage: None,
        };
        let children = self.children.values().map(|c| FlatNode {
            id: &c.id,
            parent_id: Some(&c.parent_id),
            timestamp: c.timestamp,
            message: &c.message,
            usage: c.usage.as_ref(),
        });
        std::iter::once(root).chain(children)
    }

    /// Returns the usage from the tip of the active branch, or `None` if the tip has no usage.
    ///
    /// Usage is only present on assistant nodes produced by a completed turn. A `None` return
    /// means the session ended on a user message or an interrupted assistant turn.
    pub fn last_branch_usage(&self) -> Option<&Usage> {
        self.children.get(&self.current_leaf_id)?.usage.as_ref()
    }

    /// Sums token consumption across all child nodes in the session, regardless of branch.
    pub fn total_consumption(&self) -> Usage {
        self.children.values().filter_map(|c| c.usage.clone()).fold(
            Usage::default(),
            |mut acc, u| {
                acc += u;
                acc
            },
        )
    }

    /// Builds and returns the session tree for UI rendering.
    pub fn as_tree(&self) -> SessionTree {
        let stem = build_branch(self);
        SessionTree {
            current_leaf_id: self.current_leaf_id.clone(),
            stem,
        }
    }
}

/// A unified read-only view of either a [`RootNode`] or a [`ChildNode`].
///
/// Used for tree-building and iteration without allocating a unified node type.
pub struct FlatNode<'a> {
    pub id: &'a NodeId,
    pub parent_id: Option<&'a NodeId>,
    pub timestamp: Timestamp,
    pub message: &'a Message,
    pub usage: Option<&'a Usage>,
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
    fn create_seeds_first_message() {
        let (_tmp, r) = registry();
        let session = r.create_session(Message::user("hello")).unwrap();
        assert_eq!(session.current_leaf_id, session.root.id);
        assert_eq!(session.history().len(), 1);
        assert_eq!(session.history()[0], Message::user("hello"));
    }

    #[test]
    fn append_advances_leaf_and_persists() {
        let (_tmp, r) = registry();
        let mut session = r.create_session(Message::user("hello")).unwrap();

        session.append_message(Message::assistant("hi")).unwrap();

        let ctx = session.history();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0], Message::user("hello"));
        assert_eq!(ctx[1], Message::assistant("hi"));
    }

    #[test]
    fn append_sets_parent_id() {
        let (_tmp, r) = registry();
        let mut session = r.create_session(Message::user("root")).unwrap();

        let first_id = session.current_leaf_id.clone();
        session.append_message(Message::assistant("reply")).unwrap();

        let leaf_id = &session.current_leaf_id;
        assert_eq!(session.children[leaf_id].parent_id, first_id);
    }

    #[test]
    fn branch_switches_active_leaf() {
        let (_tmp, r) = registry();
        let mut session = r.create_session(Message::user("root")).unwrap();

        let root_id = session.current_leaf_id.clone();
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
        let mut session = r.create_session(Message::user("root")).unwrap();
        let fake_id: NodeId = "00000000-0000-0000-0000-000000000000".parse().unwrap();
        assert!(session.branch(&fake_id).is_err());
    }

    #[test]
    fn load_restores_session_and_picks_latest_leaf() {
        let (_tmp, r) = registry();
        let session_id = {
            let mut session = r.create_session(Message::user("first")).unwrap();
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
        let id = r.create_session(Message::user("hello")).unwrap().session_id;
        let sessions = r.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
    }

    #[test]
    fn tool_result_reconstructed_as_pair() {
        let (_tmp, r) = registry();
        let session_id = {
            let mut session = r.create_session(Message::user("run it")).unwrap();
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
