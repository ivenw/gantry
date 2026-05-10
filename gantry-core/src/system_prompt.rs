use crate::resource_loader::AgentFile;

pub const BASE_PROMPT: &str = "You are an expert coding assistant operating inside a coding agent harness. You help users by reading files, executing commands, editing code, and writing new files. DON'T use emojis.";

/// Constructs the system prompt. If `agent_files` is empty, returns only the base prompt.
/// Otherwise appends a `# Context` section with each file's contents under a `## <path>` heading.
pub fn build_system_prompt(agent_files: &[AgentFile]) -> String {
    if agent_files.is_empty() {
        return BASE_PROMPT.to_string();
    }

    let mut prompt = String::from(BASE_PROMPT);
    prompt.push_str("\n\n# Context");

    for file in agent_files {
        prompt.push_str(&format!(
            "\n\n## {}\n\n{}",
            file.path.display(),
            file.contents
        ));
    }

    prompt
}
