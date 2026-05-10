use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::dirs::{AgentsDir, GlobalConfigDir, ProjectRootDir};

const CONTEXT_FILE_CANDIDATES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

/// A discovered context file with its resolved path and raw contents.
pub struct ContextFile {
    pub path: PathBuf,
    pub contents: String,
}

/// Discovers and loads context files for a given project root.
///
/// Insertion order: global first (`~/.gantry/AGENTS.md`, falling back to `~/.agents/AGENTS.md`),
/// then files found walking up from `project_root` toward the filesystem root, ordered
/// root-first so the most-root file comes before the project-level file.
pub fn load_context_files(project_root: &ProjectRootDir) -> Result<Vec<ContextFile>> {
    let mut results: Vec<ContextFile> = Vec::new();

    let global_candidates = [
        GlobalConfigDir::new().ok().map(|d| d.path().join("AGENTS.md")),
        AgentsDir::new().ok().map(|d| d.path().join("AGENTS.md")),
    ];
    for path in global_candidates.into_iter().flatten() {
        if let Ok(file) = load_context_file(&path) {
            results.push(file);
            break;
        }
    }

    let mut walk_results: Vec<ContextFile> = Vec::new();
    let mut current = project_root.path().to_path_buf();
    loop {
        for name in CONTEXT_FILE_CANDIDATES {
            if let Ok(file) = load_context_file(&current.join(name)) {
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
pub fn load_context_file(path: &Path) -> Result<ContextFile> {
    let contents = std::fs::read_to_string(path)?;
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Ok(ContextFile { path, contents })
}
