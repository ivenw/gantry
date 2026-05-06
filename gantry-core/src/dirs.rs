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
    pub fn config_path(&self) -> PathBuf {
        self.0.join("config.toml")
    }

    /// Returns the path to `~/.gantry/credentials.toml`.
    pub fn credentials_path(&self) -> PathBuf {
        self.0.join("credentials.toml")
    }
}

pub struct ProjectRootDir(PathBuf);

impl ProjectRootDir {
    pub fn new(path: &Path) -> Result<Self> {
        let path = path
            .canonicalize()
            .context("Project path does not exist.")?;
        Ok(Self(path))
    }

    /// Returns the path to the projects root directory.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

pub struct ProjectConfigDir(PathBuf);

impl ProjectConfigDir {
    pub fn new(project_dir: &ProjectRootDir) -> Result<Self> {
        let path = project_dir.path().join(".gantry");
        Ok(Self(path))
    }

    /// Returns the path to the projects config directory.
    pub fn path(&self) -> &Path {
        &self.0
    }
}
