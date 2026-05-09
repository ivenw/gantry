pub mod agent;
pub mod client;
pub mod registry;

use serde::{Deserialize, Serialize};

/// An event emitted by [`agent::ToolCallHook`] during tool execution.
#[derive(Debug, Clone)]
pub enum ToolCallEvent {
    /// Fired immediately before a tool is executed.
    Started { name: String, id: String },
    /// Fired immediately after a tool returns its result.
    Finished { id: String },
}

/// A resolved provider and model pair used to select a specific model for inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSelection {
    pub provider: ProviderAlias,
    pub model: ModelAlias,
}

/// User-defined alias for a provider instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderAlias(pub String);

impl ProviderAlias {
    /// Creates a new [`ProviderAlias`] from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// User-defined alias for a model within a provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelAlias(pub String);

impl ModelAlias {
    /// Creates a new [`ModelAlias`] from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
