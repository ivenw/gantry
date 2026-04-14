use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectInfo {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RegistryFile {
    projects: Vec<ProjectInfo>,
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
        std::fs::create_dir_all(&gantry_dir)
            .with_context(|| format!("failed to create .gantry/sessions in {}", abs_path.display()))?;

        let mut registry = self.load()?;
        let abs_str = abs_path.to_string_lossy().to_string();
        if !registry.projects.iter().any(|p| p.path == abs_str) {
            registry.projects.push(ProjectInfo { path: abs_str });
            self.save(&registry)?;
        }

        Ok(())
    }

    pub fn list(&self) -> Result<Vec<ProjectInfo>> {
        Ok(self.load()?.projects)
    }

    fn load(&self) -> Result<RegistryFile> {
        if !self.registry_path.exists() {
            return Ok(RegistryFile::default());
        }
        let contents = std::fs::read_to_string(&self.registry_path)
            .with_context(|| format!("failed to read registry at {}", self.registry_path.display()))?;
        serde_json::from_str(&contents).with_context(|| "failed to parse registry JSON")
    }

    fn save(&self, registry: &RegistryFile) -> Result<()> {
        if let Some(parent) = self.registry_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create registry dir {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(registry)?;
        std::fs::write(&self.registry_path, json)
            .with_context(|| format!("failed to write registry at {}", self.registry_path.display()))?;
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
        assert!(projects[0].path.ends_with("my_project"));
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
