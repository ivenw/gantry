use anyhow::Result;
use std::path::{Path, PathBuf};

/// Defines the contract for a project registry backend.
pub trait ProjectRegistry {
    /// Registers a project at `path`.
    fn register(&self, path: &Path) -> Result<()>;

    /// Removes a project at `path` from the registry.
    fn unregister(&self, path: &Path) -> Result<()>;

    /// Returns all registered project paths.
    fn list(&self) -> Result<Vec<PathBuf>>;
}
