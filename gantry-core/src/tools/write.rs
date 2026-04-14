use std::path::Path;

use anyhow::{Result, bail};

/// Creates a new file at `path` with the given `content`.
///
/// Returns an error if the file already exists — use `edit_file` to modify existing files.
/// Intermediate directories are created automatically.
/// An empty `content` string produces a 0-byte file.
pub fn write_file(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        bail!(
            "file already exists: {}; use edit_file to modify existing files",
            path.display()
        );
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn creates_file_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        write_file(&path, "hello world").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn empty_content_creates_zero_byte_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.txt");
        write_file(&path, "").unwrap();
        assert!(path.exists());
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
    }

    #[test]
    fn errors_if_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        fs::write(&path, "original").unwrap();
        let err = write_file(&path, "new content").unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert!(err.to_string().contains("edit_file"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    }

    #[test]
    fn creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/c.txt");
        write_file(&path, "deep").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "deep");
    }
}
