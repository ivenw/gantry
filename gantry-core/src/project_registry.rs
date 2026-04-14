use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
struct RegistryFile {
    projects: Vec<PathBuf>,
}

pub struct ProjectRegistry {
    registry_path: PathBuf,
}

impl ProjectRegistry {
    pub fn new(registry_path: PathBuf) -> Self {
        Self { registry_path }
    }

    pub fn register(&self, path: &Path) -> Result<()> {
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

        let mut registry = self.load()?;
        if !registry.projects.contains(&abs_path) {
            registry.projects.push(abs_path);
            self.save(&registry)?;
        }

        Ok(())
    }

    pub fn unregister(&self, path: &Path) -> Result<()> {
        let mut registry = self.load()?;
        let before = registry.projects.len();
        registry.projects.retain(|p| p != path);
        if registry.projects.len() == before {
            anyhow::bail!("project not found in registry: {}", path.display());
        }
        self.save(&registry)
    }

    pub fn list(&self) -> Result<Vec<PathBuf>> {
        Ok(self.load()?.projects)
    }

    fn load(&self) -> Result<RegistryFile> {
        if !self.registry_path.exists() {
            return Ok(RegistryFile::default());
        }
        let contents = std::fs::read_to_string(&self.registry_path).with_context(|| {
            format!(
                "failed to read registry at {}",
                self.registry_path.display()
            )
        })?;
        serde_json::from_str(&contents).with_context(|| "failed to parse registry JSON")
    }

    fn save(&self, registry: &RegistryFile) -> Result<()> {
        if let Some(parent) = self.registry_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create registry dir {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(registry)?;
        std::fs::write(&self.registry_path, json).with_context(|| {
            format!(
                "failed to write registry at {}",
                self.registry_path.display()
            )
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, ProjectRegistry) {
        let tmp = TempDir::new().unwrap();
        let registry_path = tmp.path().join(".gantry").join("projects.json");
        let registry = ProjectRegistry::new(registry_path);
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
