use rig::completion::Usage;

/// Snapshot of context window consumption for the most recent request.
#[derive(Clone)]
pub struct ContextWindow {
    /// Total tokens used in the last request (input + output).
    pub total_tokens: u64,
    /// Maximum tokens the model's context window can hold, if known.
    pub context_length: Option<u32>,
}

impl ContextWindow {
    pub fn new(usage: &Usage, context_length: Option<u32>) -> Self {
        Self {
            total_tokens: usage.total_tokens,
            context_length,
        }
    }

    /// Tokens remaining in the context window, if the context length is known.
    pub fn remaining_tokens(&self) -> Option<u64> {
        self.context_length
            .map(|ctx| (ctx as u64).saturating_sub(self.total_tokens))
    }

    /// Fraction of the context window consumed (0.0–1.0), if the context length is known.
    pub fn usage_fraction(&self) -> Option<f32> {
        self.context_length
            .map(|ctx| self.total_tokens as f32 / ctx as f32)
    }

    /// Percentage of the context window consumed (0–100), if the context length is known.
    pub fn usage_percent(&self) -> Option<f32> {
        self.usage_fraction().map(|f| f * 100.0)
    }
}
