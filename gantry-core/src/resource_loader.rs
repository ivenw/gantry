use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::dirs::{AgentsDir, GlobalConfigDir, ProjectRootDir};

const CONTEXT_FILE_CANDIDATES: &[&str] = &["AGENTS.md", "CLAUDE.md"];
const SKILL_FILE_NAME: &str = "SKILL.md";
const SKIP_DIRS: &[&str] = &[".git", "node_modules"];

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

/// Parsed metadata from a `SKILL.md` frontmatter block.
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

/// A discovered skill with its parsed metadata and the absolute path to its `SKILL.md`.
pub struct Skill {
    pub metadata: SkillMetadata,
    /// Absolute path to the `SKILL.md` file.
    pub skill_file: PathBuf,
}

/// Discovers and loads skills for a given project root.
///
/// Scans four locations in priority order (user-global first, then project-level). Project-level
/// skills shadow user-level skills that share the same `name`. Within the same scope,
/// first-found wins. A warning is logged when a collision occurs.
///
/// Scan order:
/// 1. `~/.gantry/skills/`
/// 2. `~/.agents/skills/`
/// 3. `<project>/.gantry/skills/`
/// 4. `<project>/.agents/skills/`
pub fn load_skills(project_root: &ProjectRootDir) -> Result<Vec<Skill>> {
    // Collect skills in ascending priority order so that higher-priority entries overwrite lower.
    // Pair each dir with a flag indicating whether it is project-level (higher priority).
    let scan_dirs: Vec<(PathBuf, bool)> = [
        GlobalConfigDir::new().ok().map(|d| (d.skills_dir(), false)),
        AgentsDir::new().ok().map(|d| (d.skills_dir(), false)),
        Some((project_root.config_dir().skills_dir(), true)),
        Some((project_root.agents_skills_dir(), true)),
    ]
    .into_iter()
    .flatten()
    .collect();

    // user-level entries first, project-level second — later entries win on collision.
    let mut by_name: HashMap<String, Skill> = HashMap::new();
    for (dir, _is_project) in scan_dirs {
        for skill in scan_skills_dir(&dir) {
            if let Some(existing) = by_name.get(&skill.metadata.name) {
                eprintln!(
                    "gantry: skill '{}' found at both '{}' and '{}'; the latter takes precedence",
                    skill.metadata.name,
                    existing.skill_file.display(),
                    skill.skill_file.display(),
                );
            }
            by_name.insert(skill.metadata.name.clone(), skill);
        }
    }

    Ok(by_name.into_values().collect())
}

/// Scans one `skills/` directory, returning all successfully parsed skills.
///
/// Each immediate subdirectory (except `.git` and `node_modules`) is checked for a `SKILL.md`.
/// Parse failures are logged and skipped.
fn scan_skills_dir(dir: &Path) -> Vec<Skill> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if SKIP_DIRS.contains(&dir_name) {
            continue;
        }
        let skill_file = path.join(SKILL_FILE_NAME);
        match parse_skill_file(&skill_file) {
            Ok(skill) => skills.push(skill),
            Err(e) => eprintln!(
                "gantry: skipping skill at '{}': {e}",
                skill_file.display()
            ),
        }
    }
    skills
}

#[derive(Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// Reads and parses a `SKILL.md` file, extracting `name` and `description` from its YAML
/// frontmatter.
///
/// Returns an error if the file is unreadable, the YAML is unparseable, or `description` is
/// missing. Warns (but still loads) if `name` does not match the parent directory name. Applies a
/// lenient fallback for YAML values that contain bare colons (a common cross-client compatibility
/// issue).
pub fn parse_skill_file(path: &Path) -> Result<Skill> {
    let raw = std::fs::read_to_string(path)?;

    let (name, description) = extract_frontmatter(&raw, path)?;

    let skill_file = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Warn if the name doesn't match the parent directory.
    let dir_name = skill_file
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if name != dir_name {
        eprintln!(
            "gantry: skill name '{}' does not match directory '{}' in '{}'",
            name,
            dir_name,
            skill_file.display()
        );
    }

    Ok(Skill {
        metadata: SkillMetadata { name, description },
        skill_file,
    })
}

/// Extracts `name` and `description` from the YAML frontmatter of a `SKILL.md` file.
///
/// Tries strict parsing first, then falls back to a lenient approach that wraps bare colon-
/// containing description values in quotes before retrying.
fn extract_frontmatter(raw: &str, path: &Path) -> Result<(String, String)> {
    let yaml = slice_frontmatter(raw)
        .ok_or_else(|| anyhow::anyhow!("no YAML frontmatter found"))?;

    let fm: SkillFrontmatter = serde_yaml::from_str(yaml)
        .or_else(|_| serde_yaml::from_str(&lenient_yaml(yaml)))
        .map_err(|e| anyhow::anyhow!("could not parse frontmatter in '{}': {e}", path.display()))?;

    let name = fm.name.filter(|s| !s.is_empty()).unwrap_or_else(|| {
        // Fall back to the parent directory name.
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let description = fm
        .description
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing or empty description in '{}'", path.display()))?;

    Ok((name, description))
}

/// Returns the YAML block between the first pair of `---` delimiters, or `None` if not found.
fn slice_frontmatter(raw: &str) -> Option<&str> {
    let rest = raw.strip_prefix("---")?;
    // Accept `---\n` or `---\r\n`
    let rest = rest.strip_prefix('\n').or_else(|| rest.strip_prefix("\r\n"))?;
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

/// Wraps bare `description:` values that contain colons in double quotes so that strict YAML
/// parsers can handle them. Only touches the `description` line.
fn lenient_yaml(yaml: &str) -> String {
    yaml.lines()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("description:") {
                let value = rest.trim();
                if value.contains(':') && !value.starts_with('"') && !value.starts_with('\'') {
                    return format!("description: \"{value}\"");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_skill(dir: &TempDir, subdir: &str, contents: &str) -> PathBuf {
        let skill_dir = dir.path().join(subdir);
        fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn parse_valid_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let path = write_skill(
            &tmp,
            "my-skill",
            "---\nname: my-skill\ndescription: Does something useful\n---\n\n# Body\n",
        );
        let skill = parse_skill_file(&path).unwrap();
        assert_eq!(skill.metadata.name, "my-skill");
        assert_eq!(skill.metadata.description, "Does something useful");
    }

    #[test]
    fn parse_missing_description_returns_error() {
        let tmp = TempDir::new().unwrap();
        let path = write_skill(&tmp, "no-desc", "---\nname: no-desc\n---\n\n# Body\n");
        assert!(parse_skill_file(&path).is_err());
    }

    #[test]
    fn parse_empty_description_returns_error() {
        let tmp = TempDir::new().unwrap();
        let path = write_skill(
            &tmp,
            "empty-desc",
            "---\nname: empty-desc\ndescription: \n---\n\n# Body\n",
        );
        assert!(parse_skill_file(&path).is_err());
    }

    #[test]
    fn parse_unparseable_yaml_returns_error() {
        let tmp = TempDir::new().unwrap();
        let path = write_skill(&tmp, "bad-yaml", "---\n: : :\n---\n");
        assert!(parse_skill_file(&path).is_err());
    }

    #[test]
    fn parse_lenient_colon_in_description() {
        let tmp = TempDir::new().unwrap();
        let path = write_skill(
            &tmp,
            "lenient",
            "---\nname: lenient\ndescription: Use this when: the user asks something\n---\n",
        );
        let skill = parse_skill_file(&path).unwrap();
        assert_eq!(
            skill.metadata.description,
            "Use this when: the user asks something"
        );
    }

    #[test]
    fn parse_no_frontmatter_returns_error() {
        let tmp = TempDir::new().unwrap();
        let path = write_skill(&tmp, "no-fm", "# Just a markdown file\n");
        assert!(parse_skill_file(&path).is_err());
    }
}
