use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::dirs::GlobalConfigDir;
use crate::project::registry::ProjectRegistry;

/// Persists the set of registered project paths to a JSON file on disk.
pub struct FsProjectRegistry {
    registry_path: PathBuf,
}

impl FsProjectRegistry {
    /// Creates a new registry stored inside `global_config_dir`, creating it if it does not exist.
    pub fn new(global_config_dir: &GlobalConfigDir) -> Result<Self> {
        let path = global_config_dir.path();
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create data dir {}", path.display()))?;
        let registry_path = path
            .canonicalize()
            .with_context(|| format!("data dir does not exist: {}", path.display()))?
            .join("projects.json");
        Ok(Self { registry_path })
    }

    /// Deserializes the registry file, returning an empty list if it does not yet exist.
    fn load(&self) -> Result<Vec<PathBuf>> {
        if !self.registry_path.exists() {
            return Ok(Vec::new());
        }
        let contents = std::fs::read_to_string(&self.registry_path).with_context(|| {
            format!(
                "failed to read registry at {}",
                self.registry_path.display()
            )
        })?;
        serde_json::from_str(&contents).with_context(|| "failed to parse registry JSON")
    }

    /// Serializes `projects` and writes them to the registry file.
    fn save(&self, projects: &[PathBuf]) -> Result<()> {
        let json = serde_json::to_string_pretty(projects)?;
        std::fs::write(&self.registry_path, json).with_context(|| {
            format!(
                "failed to write registry at {}",
                self.registry_path.display()
            )
        })?;
        Ok(())
    }
}

impl ProjectRegistry for FsProjectRegistry {
    /// Registers a project at `path`, creating `.gantry/sessions` inside it and adding it to the registry.
    ///
    /// Idempotent — calling with an already-registered path is a no-op.
    fn register(&self, path: &Path) -> Result<()> {
        let abs_path = path
            .canonicalize()
            .with_context(|| format!("path does not exist: {}", path.display()))?;

        let gantry_dir = abs_path.join(".gantry").join("sessions");
        std::fs::create_dir_all(&gantry_dir).with_context(|| {
            format!(
                "failed to create .gantry/sessions in {}",
                abs_path.display()
            )
        })?;

        let mut projects = self.load()?;
        if !projects.contains(&abs_path) {
            projects.push(abs_path);
            self.save(&projects)?;
        }

        Ok(())
    }

    /// Removes `path` from the registry. Returns an error if the path is not registered.
    fn unregister(&self, path: &Path) -> Result<()> {
        let mut projects = self.load()?;
        let before = projects.len();
        projects.retain(|p| p != path);
        if projects.len() == before {
            anyhow::bail!("project not found in registry: {}", path.display());
        }
        self.save(&projects)
    }

    /// Returns all registered project paths.
    fn list(&self) -> Result<Vec<PathBuf>> {
        self.load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsProjectRegistry) {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path().join(".gantry");
        std::fs::create_dir_all(&data_dir).unwrap();
        let registry_path = data_dir.canonicalize().unwrap().join("projects.json");
        let registry = FsProjectRegistry { registry_path };
        (tmp, registry)
    }

    #[test]
    fn list_returns_empty_when_no_registry_file() {
        let (_tmp, registry) = setup();
        let projects = registry.list().unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn register_creates_gantry_sessions_dir() {
        let (tmp, registry) = setup();
        let project_dir = tmp.path().join("my_project");
        std::fs::create_dir(&project_dir).unwrap();

        registry.register(&project_dir).unwrap();

        assert!(project_dir.join(".gantry").join("sessions").exists());
    }

    #[test]
    fn register_adds_project_to_list() {
        let (tmp, registry) = setup();
        let project_dir = tmp.path().join("my_project");
        std::fs::create_dir(&project_dir).unwrap();

        registry.register(&project_dir).unwrap();

        let projects = registry.list().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].file_name().unwrap(), "my_project");
    }

    #[test]
    fn register_is_idempotent() {
        let (tmp, registry) = setup();
        let project_dir = tmp.path().join("my_project");
        std::fs::create_dir(&project_dir).unwrap();

        registry.register(&project_dir).unwrap();
        registry.register(&project_dir).unwrap();

        let projects = registry.list().unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[test]
    fn register_multiple_projects() {
        let (tmp, registry) = setup();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();

        registry.register(&a).unwrap();
        registry.register(&b).unwrap();

        let projects = registry.list().unwrap();
        assert_eq!(projects.len(), 2);
    }

    #[test]
    fn register_fails_for_nonexistent_path() {
        let (tmp, registry) = setup();
        let missing = tmp.path().join("does_not_exist");

        let result = registry.register(&missing);
        assert!(result.is_err());
    }
}
