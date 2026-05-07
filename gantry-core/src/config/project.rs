use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The on-disk representation of `gantry.toml`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
}

impl ProjectConfig {
    /// Loads and parses the project config from `path`.
    ///
    /// Returns an error if the file does not exist or cannot be parsed.
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&contents).context("failed to parse gantry.toml")
    }

    /// Writes a new project config to `path`.
    ///
    /// The project name is derived from the git remote origin (`owner/repo`), falling back to
    /// the directory name. Returns an error if the file already exists or cannot be written.
    pub fn init(path: &Path) -> Result<()> {
        if path.exists() {
            anyhow::bail!("gantry.toml already exists at {}", path.display());
        }
        let project_root = path.parent().unwrap_or(path);
        let name = resolve_project_name(project_root);

        let mut doc = toml_edit::DocumentMut::new();
        doc["name"] = toml_edit::value(name);

        std::fs::write(path, doc.to_string())
            .with_context(|| format!("failed to write gantry.toml at {}", path.display()))
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
