use std::fmt;
use std::path::{Path, PathBuf};

use crate::agentsmd::load_agentsmd_files;
use crate::skills::load_skills;

const SYSTEM_MD_NAME: &str = "SYSTEM.md";
const DEFAULT_BASE_PROMPT: &str = include_str!("../SYSTEM.md");

const SKILL_ACTIVATION_INSTRUCTIONS: &str = "\
The following skills provide specialized instructions for specific tasks.
When a task matches a skill's description, use your file-read tool to load \
the SKILL.md at the listed location before proceeding.
When a skill references relative paths, resolve them against the skill's \
directory (the parent of SKILL.md) and use absolute paths in tool calls.";

/// Owns the built system prompt string and the per-component char counts used for context window
/// estimation.
///
/// Call [`SystemPrompt::new`] to construct from cwd, and [`SystemPrompt::refresh`] to rebuild
/// in place when the underlying files change.
pub struct SystemPrompt {
    prompt: String,
    base_prompt_char_count: usize,
    agent_file_char_counts: Vec<(PathBuf, usize)>,
    skills_catalog_char_count: usize,
}

impl SystemPrompt {
    /// Discovers `SYSTEM.md`, agent files, and skills under `cwd` and builds the system prompt.
    pub fn new(cwd: &Path) -> Self {
        let base_prompt = load_base_prompt(cwd);
        let base_prompt_len = base_prompt.len();

        let agent_files = load_agentsmd_files(cwd).unwrap_or_default();
        let skills = load_skills(cwd).unwrap_or_default();

        let agent_file_char_counts = agent_files
            .iter()
            .map(|f| (f.path.clone(), f.contents.len()))
            .collect();
        let skills_catalog_char_count = skills
            .iter()
            .map(|s| s.metadata.name.len() + s.metadata.description.len())
            .sum();

        let prompt = build_prompt(&base_prompt, &agent_files, &skills);

        Self {
            prompt,
            base_prompt_char_count: base_prompt_len,
            agent_file_char_counts,
            skills_catalog_char_count,
        }
    }

    /// Rediscovers all files and rebuilds the prompt in place.
    pub fn refresh(&mut self, cwd: &Path) {
        *self = Self::new(cwd);
    }

    /// Returns the char count of the base prompt component.
    pub fn base_prompt_char_count(&self) -> usize {
        self.base_prompt_char_count
    }

    /// Returns the char count per discovered agent file, in the order they appear in the prompt.
    pub fn agent_file_char_counts(&self) -> &[(PathBuf, usize)] {
        &self.agent_file_char_counts
    }

    /// Returns the total chars contributed by the skills catalog section.
    pub fn skills_catalog_char_count(&self) -> usize {
        self.skills_catalog_char_count
    }
}

impl fmt::Display for SystemPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.prompt)
    }
}

/// Walks up from `cwd` looking for the first `SYSTEM.md`, falling back to the compiled-in
/// default if none is found.
fn load_base_prompt(cwd: &Path) -> String {
    let mut current = cwd.to_path_buf();
    loop {
        let candidate = current.join(SYSTEM_MD_NAME);
        if let Ok(contents) = std::fs::read_to_string(&candidate) {
            return contents;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return DEFAULT_BASE_PROMPT.to_string(),
        }
    }
}

fn build_prompt(
    base_prompt: &str,
    agent_files: &[crate::agentsmd::AgentsMdFile],
    skills: &[crate::skills::Skill],
) -> String {
    let mut prompt = base_prompt.trim_end().to_string();

    if !agent_files.is_empty() {
        prompt.push_str("\n\n# Context");
        for file in agent_files {
            prompt.push_str(&format!(
                "\n\n## {}\n\n{}",
                file.path.display(),
                file.contents
            ));
        }
    }

    if !skills.is_empty() {
        prompt.push_str("\n\n# Skills\n\n");
        prompt.push_str(SKILL_ACTIVATION_INSTRUCTIONS);
        prompt.push_str("\n\n<available_skills>");
        for skill in skills {
            prompt.push_str(&format!(
                "\n  <skill>\n    <name>{}</name>\n    <description>{}</description>\n    <location>{}</location>\n  </skill>",
                skill.metadata.name,
                skill.metadata.description,
                skill.skill_file.display(),
            ));
        }
        prompt.push_str("\n</available_skills>");
    }

    prompt
}
