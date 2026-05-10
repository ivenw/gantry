use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const GANTRY_DIR: &str = ".gantry";
const AGENTS_DIR: &str = ".agents";
const SKILL_DIR: &str = "skills";

const GLOBAL_CONFIG_FILE: &str = "config.toml";
const CREDENTIALS_FILE: &str = "credentials.toml";

const PROJECT_CONFIG_FILE: &str = "gantry.toml";

/// The application's config directory, resolved from the OS home dir with `/.gantry` appended.
pub struct GlobalGantryDir(PathBuf);

impl GlobalGantryDir {
    /// Resolves the config directory, returning an error if the OS home dir is unavailable.
    pub fn new() -> Result<Self> {
        let path = dirs::home_dir()
            .context("Could not resolve OS home directory")?
            .join(GANTRY_DIR);
        Ok(Self(path))
    }

    /// Returns the path to the config directory.
    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.0.join(SKILL_DIR)
    }

    /// Returns the path to `~/.gantry/config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.0.join(GLOBAL_CONFIG_FILE)
    }

    /// Returns the path to `~/.gantry/credentials.toml`.
    pub fn credentials_file(&self) -> PathBuf {
        self.0.join(CREDENTIALS_FILE)
    }

    /// Returns the path to `~/.gantry/sessions/<project_name>/`.
    ///
    /// Slashes in `project_name` (e.g. `owner/repo`) are replaced with `_` so the name maps to a
    /// single directory component.
    pub fn sessions_dir(&self, project_name: &str) -> PathBuf {
        let sanitized = project_name.replace('/', "_");
        self.0.join("sessions").join(sanitized)
    }
}

/// A global configuration directory for agents.
pub struct GlobalAgentsDir(PathBuf);

impl GlobalAgentsDir {
    /// Resolves the config directory, returning an error if the OS home dir is unavailable.
    pub fn new() -> Result<Self> {
        let path = dirs::home_dir()
            .context("Could not resolve OS home directory")?
            .join(AGENTS_DIR);
        Ok(Self(path))
    }

    /// Returns the path to the agents directory.
    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.0.join(SKILL_DIR)
    }
}

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
    pub fn gantry_dir(&self) -> ProjectGantryDir {
        ProjectGantryDir::new(self)
    }

    /// Returns the `.agents/` directory inside this project root.
    pub fn agents_dir(&self) -> ProjectAgentsDir {
        ProjectAgentsDir::new(self)
    }
}

/// The `.agents/` directory inside a project root (cross-client convention).
pub struct ProjectAgentsDir(PathBuf);

impl ProjectAgentsDir {
    /// Constructs the agents directory path from a project root.
    fn new(project_root: &ProjectRootDir) -> Self {
        Self(project_root.path().join(AGENTS_DIR))
    }

    /// Returns the path to the project agents directory.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Returns the path to `<project_root>/.agents/skills/`.
    pub fn skills_dir(&self) -> PathBuf {
        self.0.join(SKILL_DIR)
    }
}

/// The `.gantry/` config directory inside a project root.
pub struct ProjectGantryDir(PathBuf);

impl ProjectGantryDir {
    /// Constructs the config directory path from a project root.
    fn new(project_root: &ProjectRootDir) -> Self {
        Self(project_root.path().join(GANTRY_DIR))
    }

    /// Returns the path to the project config directory.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Returns the path to `<project_root>/.gantry/skills/`.
    pub fn skills_dir(&self) -> PathBuf {
        self.0.join(SKILL_DIR)
    }
}
