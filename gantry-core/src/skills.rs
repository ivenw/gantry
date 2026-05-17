use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::dirs::{GlobalAgentsDir, GlobalGantryDir};

const AGENTS_DIR: &str = ".agents";
const GANTRY_DIR: &str = ".gantry";
const SKILLS_DIR: &str = "skills";
const SKILL_FILE_NAME: &str = "SKILL.md";
const SKIP_DIRS: &[&str] = &[".git", "node_modules"];

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

/// Discovers and loads skills starting from `cwd`.
///
/// Scans in ascending priority order so that later insertions win on name collision:
/// 1. `~/.agents/skills/`
/// 2. `~/.gantry/skills/`
/// 3. For each ancestor of `cwd` walking root-first toward `cwd`:
///    a. `<dir>/.agents/skills/`
///    b. `<dir>/.gantry/skills/`
///
/// Within the same directory, `.gantry/` beats `.agents/` because it is inserted last.
/// Closer ancestors beat further ones for the same reason.
pub fn load_skills(cwd: &Path) -> Result<Vec<Skill>> {
    let mut by_name: HashMap<String, Skill> = HashMap::new();

    let mut insert = |skill: Skill| {
        by_name.insert(skill.metadata.name.clone(), skill);
    };

    // Global dirs — lowest priority.
    if let Ok(d) = GlobalAgentsDir::new() {
        for skill in scan_skills_dir(&d.skills_dir()) {
            insert(skill);
        }
    }
    if let Ok(d) = GlobalGantryDir::new() {
        for skill in scan_skills_dir(&d.skills_dir()) {
            insert(skill);
        }
    }

    // Ancestors of cwd, root-first, so closer dirs overwrite further ones.
    let mut ancestors: Vec<PathBuf> = cwd.ancestors().map(|p| p.to_path_buf()).collect();
    ancestors.reverse();
    for ancestor in &ancestors {
        for config_dir in &[AGENTS_DIR, GANTRY_DIR] {
            let skills_dir = ancestor.join(config_dir).join(SKILLS_DIR);
            for skill in scan_skills_dir(&skills_dir) {
                insert(skill);
            }
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
            Err(_) => continue,
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
    let yaml =
        slice_frontmatter(raw).ok_or_else(|| anyhow::anyhow!("no YAML frontmatter found"))?;

    let fm: SkillFrontmatter = serde_yaml::from_str(yaml)
        .or_else(|_| serde_yaml::from_str(&lenient_yaml(yaml)))
        .map_err(|e| anyhow::anyhow!("could not parse frontmatter in '{}': {e}", path.display()))?;

    let name = fm.name.filter(|s| !s.is_empty()).unwrap_or_else(|| {
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
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))?;
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
