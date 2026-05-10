use rig::completion::Usage as RigUsage;
use serde::{Deserialize, Serialize};

/// Token usage for a single completed request, stored on the assistant node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Input tokens served from the provider's prompt cache.
    pub cached_input_tokens: u64,
    /// Input tokens written to the provider's prompt cache.
    pub cache_creation_input_tokens: u64,
}

impl Usage {
    /// Total tokens (input + output) for this request.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

impl std::ops::AddAssign for Usage {
    /// Adds `rhs` field-by-field into `self`.
    fn add_assign(&mut self, rhs: Self) {
        self.input_tokens += rhs.input_tokens;
        self.output_tokens += rhs.output_tokens;
        self.cached_input_tokens += rhs.cached_input_tokens;
        self.cache_creation_input_tokens += rhs.cache_creation_input_tokens;
    }
}

impl From<&RigUsage> for Usage {
    fn from(u: &RigUsage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cached_input_tokens: u.cached_input_tokens,
            cache_creation_input_tokens: u.cache_creation_input_tokens,
        }
    }
}
