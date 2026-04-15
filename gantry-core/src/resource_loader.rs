use std::path::{Path, PathBuf};

/// Discovers and loads AGENTS.md files for a given project path.
///
/// Final insertion order: global first (`~/.gantry/AGENTS.md`), then files found
/// walking up from `project_path` toward the filesystem root, reversed so the
/// most-root file comes before the project-level file.
pub fn discover_agents_md(project_path: &Path) -> Vec<(PathBuf, String)> {
    let mut results: Vec<(PathBuf, String)> = Vec::new();

    if let Some(global_path) = global_agents_md_path()
        && let Ok(contents) = std::fs::read_to_string(&global_path)
    {
        results.push((global_path, contents));
    }

    let mut walk_results: Vec<(PathBuf, String)> = Vec::new();
    let mut current = project_path.to_path_buf();
    loop {
        let candidate = find_context_file(&current);
        if let Some((path, contents)) = candidate {
            walk_results.push((path, contents));
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

/// Returns `(path, contents)` for the first context file found in `dir`.
/// Prefers `AGENTS.md`, falls back to `CLAUDE.md`.
fn find_context_file(dir: &Path) -> Option<(PathBuf, String)> {
    for name in CONTEXT_FILE_CANDIDATES {
        let candidate = dir.join(name);
        if let Ok(contents) = std::fs::read_to_string(&candidate) {
            let display_path = candidate.canonicalize().unwrap_or(candidate);
            return Some((display_path, contents));
        }
    }
    None
}

fn global_agents_md_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".gantry").join("AGENTS.md"))
}
