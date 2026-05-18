use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::dirs::{GlobalAgentsDir, GlobalGantryDir};

const AGENTS_MD_FILE_CANDIDATES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

/// A discovered context file with its resolved path and raw contents.
pub struct AgentsMdFile {
    pub path: PathBuf,
    pub contents: String,
}

/// Discovers and loads context files starting from `cwd`.
///
/// Insertion order: global first (`~/.gantry/AGENTS.md`, falling back to `~/.agents/AGENTS.md`),
/// then files found walking up from `cwd` toward the filesystem root, ordered root-first so the
/// most-root file comes before the nearest file.
pub fn load_agentsmd_files(cwd: &Path) -> Result<Vec<AgentsMdFile>> {
    let mut results: Vec<AgentsMdFile> = Vec::new();

    let global_candidates = [
        GlobalGantryDir::new()
            .ok()
            .map(|d| d.path().join("AGENTS.md")),
        GlobalAgentsDir::new()
            .ok()
            .map(|d| d.path().join("AGENTS.md")),
    ];
    for path in global_candidates.into_iter().flatten() {
        if let Ok(file) = load_agentsmd_file(&path) {
            results.push(file);
            break;
        }
    }

    let mut walk_results: Vec<AgentsMdFile> = Vec::new();
    let mut current = cwd.to_path_buf();
    loop {
        for name in AGENTS_MD_FILE_CANDIDATES {
            if let Ok(file) = load_agentsmd_file(&current.join(name)) {
                walk_results.push(file);
                break;
            }
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    walk_results.reverse();
    results.extend(walk_results);
    Ok(results)
}

/// Loads a single context file from `path`, returning an error if it does not exist or is
/// unreadable.
fn load_agentsmd_file(path: &Path) -> Result<AgentsMdFile> {
    let contents = std::fs::read_to_string(path)?;
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Ok(AgentsMdFile { path, contents })
}
