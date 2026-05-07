use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "gantry.toml";

/// A gantry project, identified by a stable name.
///
/// The name is the canonical identity used to group sessions, including on a central server.
/// It is read from `gantry.toml` in the project root, which must be committed to version control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub name: String,
}

/// The on-disk representation of `gantry.toml`.
#[derive(Debug, Serialize, Deserialize)]
struct ProjectConfig {
    name: String,
}

impl Project {
    /// Loads a project from `gantry.toml` at `project_path`.
    ///
    /// Returns an error if the file is missing (user must run `gantry init`) or malformed.
    pub fn load(project_path: &Path) -> Result<Self> {
        let config_path = project_path.join(CONFIG_FILE);
        let contents = std::fs::read_to_string(&config_path).with_context(|| {
            format!(
                "no gantry.toml found at {} — run `gantry init` to initialize the project",
                project_path.display()
            )
        })?;
        let config: ProjectConfig =
            toml::from_str(&contents).context("failed to parse gantry.toml")?;
        Ok(Self { name: config.name })
    }

    /// Initializes a new project at `project_path` by writing `gantry.toml`.
    ///
    /// The project name is derived from the git remote origin (`owner/repo`), falling back to
    /// the directory name. Returns an error if the file already exists or cannot be written.
    pub fn init(project_path: &Path) -> Result<Self> {
        let config_path = project_path.join(CONFIG_FILE);
        if config_path.exists() {
            anyhow::bail!("gantry.toml already exists at {}", project_path.display());
        }
        let name = resolve_project_name(project_path);
        let config = ProjectConfig { name: name.clone() };
        let contents =
            toml::to_string_pretty(&config).context("failed to serialize gantry.toml")?;
        std::fs::write(&config_path, contents)
            .with_context(|| format!("failed to write gantry.toml at {}", config_path.display()))?;
        Ok(Self { name })
    }
}

/// Derives the project name from the git remote origin, falling back to the directory name.
///
/// Parses both HTTPS (`https://host/owner/repo`) and SSH (`git@host:owner/repo`) remote URLs,
/// extracting `owner/repo` with any `.git` suffix stripped. Falls back to the directory name
/// if no remote origin is found or the URL cannot be parsed.
fn resolve_project_name(project_path: &Path) -> String {
    if let Some(name) = name_from_git_origin(project_path) {
        return name;
    }
    project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string()
}

fn name_from_git_origin(project_path: &Path) -> Option<String> {
    let repo = git2::Repository::discover(project_path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    let url = remote.url()?;
    parse_owner_repo(url)
}

/// Extracts `owner/repo` from an HTTPS or SSH git remote URL.
fn parse_owner_repo(url: &str) -> Option<String> {
    // SSH: git@host:owner/repo.git  or  ssh://git@host/owner/repo.git
    // HTTPS: https://host/owner/repo.git
    let path_part = if url.contains("://") {
        url.splitn(2, "://")
            .nth(1)
            .and_then(|s| s.splitn(2, '/').nth(1))?
    } else if let Some(colon_pos) = url.find(':') {
        &url[colon_pos + 1..]
    } else {
        return None;
    };

    let stripped = path_part.trim_end_matches(".git");
    let parts: Vec<&str> = stripped.splitn(3, '/').collect();
    if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https_url() {
        assert_eq!(
            parse_owner_repo("https://github.com/acme/myrepo.git"),
            Some("acme/myrepo".to_string())
        );
    }

    #[test]
    fn parse_ssh_scp_url() {
        assert_eq!(
            parse_owner_repo("git@github.com:acme/myrepo.git"),
            Some("acme/myrepo".to_string())
        );
    }

    #[test]
    fn parse_ssh_scheme_url() {
        assert_eq!(
            parse_owner_repo("ssh://git@github.com/acme/myrepo.git"),
            Some("acme/myrepo".to_string())
        );
    }

    #[test]
    fn parse_url_without_git_suffix() {
        assert_eq!(
            parse_owner_repo("https://github.com/acme/myrepo"),
            Some("acme/myrepo".to_string())
        );
    }

    #[test]
    fn parse_url_with_subpath_ignores_extra_segments() {
        assert_eq!(
            parse_owner_repo("https://github.com/acme/myrepo/extra"),
            Some("acme/myrepo".to_string())
        );
    }

    #[test]
    fn parse_returns_none_for_bare_host() {
        assert_eq!(parse_owner_repo("https://github.com/onlyone"), None);
    }
}
