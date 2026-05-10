use rig::completion::Usage;
use serde::{Deserialize, Serialize};

/// Token usage for a single completed request, stored on the assistant node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Input tokens served from the provider's prompt cache.
    pub cached_input_tokens: u64,
    /// Input tokens written to the provider's prompt cache.
    pub cache_creation_input_tokens: u64,
}

impl RequestUsage {
    /// Total tokens (input + output) for this request.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

impl From<&Usage> for RequestUsage {
    fn from(u: &Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cached_input_tokens: u.cached_input_tokens,
            cache_creation_input_tokens: u.cache_creation_input_tokens,
        }
    }
}
