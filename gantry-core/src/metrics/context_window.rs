use std::path::PathBuf;

use rig::completion::Usage;

/// Snapshot of context window consumption for the most recent request.
#[derive(Clone)]
pub struct ContextWindow {
    /// Total tokens used in the last request (input + output).
    pub total_tokens: u64,
    /// Maximum tokens the model's context window can hold, if known.
    pub context_length: Option<u32>,
    /// Estimated tokens consumed by the base system prompt alone.
    pub base_prompt_tokens: u64,
    /// Estimated tokens consumed by each agent file, in source order.
    pub agent_files_tokens: Vec<(PathBuf, u64)>,
    /// Estimated tokens consumed by the conversation messages.
    pub messages_tokens: u64,
    /// Estimated tokens not accounted for by measured components (tool schemas, provider overhead, etc.).
    pub other_tokens: u64,
}

impl ContextWindow {
    /// Builds a `ContextWindow` from raw usage, the configured context length, and pre-request
    /// character counts. Per-component token fields are estimates scaled from char counts.
    pub fn new(usage: &Usage, context_length: Option<u32>, char_counts: &CharCounts) -> Self {
        let total_tokens = usage.total_tokens;
        let total_chars = char_counts.total();
        let scale = |chars: usize| -> u64 {
            if total_chars == 0 {
                return 0;
            }
            (chars as f64 / total_chars as f64 * total_tokens as f64).round() as u64
        };

        let base_prompt_tokens = scale(char_counts.base_prompt);
        let agent_files_tokens = char_counts
            .agent_files
            .iter()
            .map(|(path, chars)| (path.clone(), scale(*chars)))
            .collect::<Vec<_>>();
        let messages_tokens = scale(char_counts.messages);

        let accounted = base_prompt_tokens
            + agent_files_tokens.iter().map(|(_, n)| n).sum::<u64>()
            + messages_tokens;
        let other_tokens = total_tokens.saturating_sub(accounted);

        Self {
            total_tokens,
            context_length,
            base_prompt_tokens,
            agent_files_tokens,
            messages_tokens,
            other_tokens,
        }
    }

    /// Fraction of the context window consumed (0.0–1.0), if the context length is known.
    pub fn usage_fraction(&self) -> Option<f32> {
        self.context_length
            .map(|ctx| self.total_tokens as f32 / ctx as f32)
    }

    /// Tokens remaining in the context window, if the context length is known.
    pub fn remaining_tokens(&self) -> Option<u64> {
        self.context_length
            .map(|ctx| (ctx as u64).saturating_sub(self.total_tokens))
    }

    /// Fraction of the context window still available (0.0–1.0), if the context length is known.
    pub fn remaining_fraction(&self) -> Option<f32> {
        self.usage_fraction().map(|used| (1.0 - used).max(0.0))
    }

    /// Total estimated tokens for the system prompt (base prompt + all agent files).
    pub fn system_prompt_tokens(&self) -> u64 {
        self.base_prompt_tokens + self.agent_files_tokens_total()
    }

    /// Total estimated tokens across all agent files.
    pub fn agent_files_tokens_total(&self) -> u64 {
        self.agent_files_tokens.iter().map(|(_, n)| *n).sum()
    }

    /// Fraction of total tokens consumed by the system prompt (base prompt + all agent files).
    pub fn system_prompt_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.system_prompt_tokens() as f32 / self.total_tokens as f32
    }

    /// Fraction of total tokens consumed by the base prompt alone.
    pub fn base_prompt_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.base_prompt_tokens as f32 / self.total_tokens as f32
    }

    /// Fraction of total tokens consumed by each agent file, in source order.
    pub fn agent_files_fraction(&self) -> Vec<(PathBuf, f32)> {
        if self.total_tokens == 0 {
            return self
                .agent_files_tokens
                .iter()
                .map(|(p, _)| (p.clone(), 0.0))
                .collect();
        }
        self.agent_files_tokens
            .iter()
            .map(|(path, tokens)| (path.clone(), *tokens as f32 / self.total_tokens as f32))
            .collect()
    }

    /// Fraction of total tokens consumed by the conversation messages.
    pub fn messages_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.messages_tokens as f32 / self.total_tokens as f32
    }

    /// Fraction of total tokens not accounted for by measured components.
    pub fn other_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.other_tokens as f32 / self.total_tokens as f32
    }
}

/// Raw character counts per component of a request, measured before the request is sent.
pub struct CharCounts {
    pub base_prompt: usize,
    pub agent_files: Vec<(PathBuf, usize)>,
    pub messages: usize,
}

impl CharCounts {
    /// Total character count across all measured components.
    pub fn total(&self) -> usize {
        self.base_prompt + self.agent_files.iter().map(|(_, n)| n).sum::<usize>() + self.messages
    }
}
