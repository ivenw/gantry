use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The application's config directory, resolved from the OS home dir with `/.gantry` appended.
pub struct GlobalConfigDir(PathBuf);

impl GlobalConfigDir {
    /// Resolves the config directory, returning an error if the OS home dir is unavailable.
    pub fn new() -> Result<Self> {
        let path = dirs::home_dir()
            .context("Could not resolve OS home directory")?
            .join(".gantry");
        Ok(Self(path))
    }

    /// Returns the path to the config directory.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Returns the path to `~/.gantry/config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.0.join("config.toml")
    }

    /// Returns the path to `~/.gantry/credentials.toml`.
    pub fn credentials_file(&self) -> PathBuf {
        self.0.join("credentials.toml")
    }
}

const PROJECT_CONFIG_FILE: &str = "gantry.toml";

/// The root directory of a gantry project, identified by the presence of `gantry.toml`.
pub struct ProjectRootDir(PathBuf);

impl ProjectRootDir {
    /// Walks up from `path` to find the first directory containing `gantry.toml`.
    ///
    /// Returns an error if no `gantry.toml` is found in `path` or any ancestor.
    pub fn new(path: &Path) -> Result<Self> {
        let start = path
            .canonicalize()
            .context("provided path does not exist")?;
        let mut dir = start;
        loop {
            if dir.join(PROJECT_CONFIG_FILE).exists() {
                return Ok(Self(dir));
            }
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => anyhow::bail!(
                    "no gantry.toml found in the given path or any parent — run `gantry init` to initialise the project"
                ),
            }
        }
    }

    /// Returns the project root path.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Returns the path to `<project_root>/gantry.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.0.join(PROJECT_CONFIG_FILE)
    }

    /// Returns the `.gantry/` config directory inside this project root.
    pub fn config_dir(&self) -> ProjectConfigDir {
        ProjectConfigDir::new(self)
    }
}

/// The `.gantry/` config directory inside a project root.
pub struct ProjectConfigDir(PathBuf);

impl ProjectConfigDir {
    /// Constructs the config directory path from a project root.
    fn new(project_root: &ProjectRootDir) -> Self {
        Self(project_root.path().join(".gantry"))
    }

    /// Returns the path to the project config directory.
    pub fn path(&self) -> &Path {
        &self.0
    }
}
