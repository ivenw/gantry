use std::path::PathBuf;

use rig::completion::Usage;

/// Snapshot of context window consumption for the most recent request.
#[derive(Clone)]
pub struct ContextWindow {
    /// Total tokens used in the last request (input + output).
    pub total_tokens: u64,
    /// Maximum tokens the model's context window can hold, if known.
    pub context_length: Option<u32>,
    /// Per-component token breakdown.
    pub breakdown: TokenBreakdown,
}

impl ContextWindow {
    pub fn new(usage: &Usage, context_length: Option<u32>, char_counts: &CharCounts) -> Self {
        let total_tokens = usage.total_tokens;
        Self {
            total_tokens,
            context_length,
            breakdown: TokenBreakdown::from_char_counts(char_counts, total_tokens),
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

    /// Fraction of total tokens consumed by the system prompt (base prompt + all agent files).
    pub fn system_prompt_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.breakdown.system_prompt_tokens() as f32 / self.total_tokens as f32
    }

    /// Fraction of total tokens consumed by the base prompt alone.
    pub fn base_prompt_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.breakdown.base_prompt_tokens as f32 / self.total_tokens as f32
    }

    /// Fraction of total tokens consumed by each agent file, in source order.
    pub fn agent_files_fraction(&self) -> Vec<(PathBuf, f32)> {
        if self.total_tokens == 0 {
            return self
                .breakdown
                .agent_files_tokens
                .iter()
                .map(|(p, _)| (p.clone(), 0.0))
                .collect();
        }
        self.breakdown
            .agent_files_tokens
            .iter()
            .map(|(path, tokens)| (path.clone(), *tokens as f32 / self.total_tokens as f32))
            .collect()
    }

    /// Fraction of total tokens consumed by the conversation messages.
    pub fn messages_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.breakdown.messages_tokens as f32 / self.total_tokens as f32
    }

    /// Fraction of total tokens not accounted for by measured components.
    pub fn other_fraction(&self) -> f32 {
        if self.total_tokens == 0 {
            return 0.0;
        }
        self.breakdown.other_tokens as f32 / self.total_tokens as f32
    }
}

/// Estimated token counts per component, derived by scaling [`CharCounts`] by `total_tokens`.
#[derive(Clone)]
pub struct TokenBreakdown {
    pub base_prompt_tokens: u64,
    /// Per-file estimated token counts, in the same order as the source agent files.
    pub agent_files_tokens: Vec<(PathBuf, u64)>,
    pub messages_tokens: u64,
    /// Tokens not accounted for by the measured components (tool schemas, provider overhead, etc.).
    pub other_tokens: u64,
}

impl TokenBreakdown {
    fn from_char_counts(counts: &CharCounts, total_tokens: u64) -> Self {
        let total_chars = counts.total();
        let scale = |chars: usize| -> u64 {
            if total_chars == 0 {
                return 0;
            }
            (chars as f64 / total_chars as f64 * total_tokens as f64).round() as u64
        };

        let base_prompt = scale(counts.base_prompt);
        let agent_files = counts
            .agent_files
            .iter()
            .map(|(path, chars)| (path.clone(), scale(*chars)))
            .collect::<Vec<_>>();
        let messages = scale(counts.messages);

        let accounted = base_prompt + agent_files.iter().map(|(_, n)| n).sum::<u64>() + messages;
        let remainder = total_tokens.saturating_sub(accounted);

        Self {
            base_prompt_tokens: base_prompt,
            agent_files_tokens: agent_files,
            messages_tokens: messages,
            other_tokens: remainder,
        }
    }

    /// Total estimated tokens for the system prompt (base prompt + all agent files).
    pub fn system_prompt_tokens(&self) -> u64 {
        self.base_prompt_tokens + self.agent_files_tokens_total()
    }

    /// Total estimated tokens across all agent files.
    pub fn agent_files_tokens_total(&self) -> u64 {
        self.agent_files_tokens.iter().map(|(_, n)| *n).sum()
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
