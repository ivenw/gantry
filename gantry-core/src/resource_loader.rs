use std::path::{Path, PathBuf};

/// A discovered context file with its resolved path and raw contents.
pub struct AgentFile {
    pub path: PathBuf,
    pub contents: String,
}

/// Discovers and loads AGENTS.md files for a given project path.
///
/// Final insertion order: global first (`~/.gantry/AGENTS.md`), then files found
/// walking up from `project_path` toward the filesystem root, reversed so the
/// most-root file comes before the project-level file.
pub fn discover_agents_md(project_path: &Path) -> Vec<AgentFile> {
    let mut results: Vec<AgentFile> = Vec::new();

    if let Some(global_path) = global_agents_md_path()
        && let Ok(contents) = std::fs::read_to_string(&global_path)
    {
        results.push(AgentFile { path: global_path, contents });
    }

    let mut walk_results: Vec<AgentFile> = Vec::new();
    let mut current = project_path.to_path_buf();
    loop {
        if let Some(file) = find_context_file(&current) {
            walk_results.push(file);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    walk_results.reverse();
    results.extend(walk_results);
    results
}

const CONTEXT_FILE_CANDIDATES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

/// Returns an [`AgentFile`] for the first context file found in `dir`.
/// Prefers `AGENTS.md`, falls back to `CLAUDE.md`.
fn find_context_file(dir: &Path) -> Option<AgentFile> {
    for name in CONTEXT_FILE_CANDIDATES {
        let candidate = dir.join(name);
        if let Ok(contents) = std::fs::read_to_string(&candidate) {
            let path = candidate.canonicalize().unwrap_or(candidate);
            return Some(AgentFile { path, contents });
        }
    }
    None
}

fn global_agents_md_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".gantry").join("AGENTS.md"))
}
