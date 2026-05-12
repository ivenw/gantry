pub mod agent;
pub mod client;
pub mod hook;
pub mod registry;

pub use hook::{HookEvent, PromptHook};

use serde::{Deserialize, Serialize};

/// A resolved provider and model pair used to select a specific model for inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSelection {
    pub provider_alias: ProviderAlias,
    pub model_id: ModelId,
    /// Context window size in tokens, if known for this model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
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
pub struct ModelId(pub String);

impl ModelId {
    /// Creates a new [`ModelId`] from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
