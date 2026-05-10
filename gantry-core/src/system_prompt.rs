use crate::resource_loader::{ContextFile, Skill};

pub const BASE_PROMPT: &str = "You are an expert coding assistant operating inside a coding agent harness. You help users by reading files, executing commands, editing code, and writing new files. DON'T use emojis.";

const SKILL_ACTIVATION_INSTRUCTIONS: &str = "\
The following skills provide specialized instructions for specific tasks.
When a task matches a skill's description, use your file-read tool to load \
the SKILL.md at the listed location before proceeding.
When a skill references relative paths, resolve them against the skill's \
directory (the parent of SKILL.md) and use absolute paths in tool calls.";

/// Constructs the system prompt from context files and available skills.
///
/// Appends a `# Context` section (one `## <path>` heading per file) when `agent_files` is
/// non-empty, and a `# Skills` section with a machine-readable catalog when `skills` is
/// non-empty. Both sections are omitted when the respective slice is empty.
pub fn build_system_prompt(agent_files: &[ContextFile], skills: &[Skill]) -> String {
    let mut prompt = String::from(BASE_PROMPT);

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
