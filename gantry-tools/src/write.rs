use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WriteError {
    #[error("file already exists: {0}")]
    FileExists(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Creates a new file at `path` with the given `content`.
///
/// Returns an error if the file already exists.
/// Intermediate directories are created automatically.
/// An empty `content` string produces a 0-byte file.
pub fn write_file(path: &Path, content: &str) -> Result<(), WriteError> {
    if path.exists() {
        return Err(WriteError::FileExists(path.to_path_buf()));
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
        assert!(matches!(
            write_file(&path, "new content").unwrap_err(),
            WriteError::FileExists(_)
        ));
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
