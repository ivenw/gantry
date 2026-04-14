use std::path::{Path, PathBuf};

use rust_tree::rust_tree::{options::TreeOptions, traversal::list_directory_as_string};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TreeError {
    #[error("path does not exist: {0}")]
    PathNotFound(PathBuf),
    #[error("path is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("failed to list directory {path}: {source}")]
    ListFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Lists a directory tree as a string.
///
/// `depth` limits how many levels deep to recurse (`None` = unlimited).
pub fn tree(path: &Path, depth: Option<u32>) -> Result<String, TreeError> {
    if !path.exists() {
        return Err(TreeError::PathNotFound(path.to_path_buf()));
    }
    if !path.is_dir() {
        return Err(TreeError::NotADirectory(path.to_path_buf()));
    }

    let options = TreeOptions {
        all_files: false,
        level: depth,
        full_path: false,
        dir_only: false,
        no_indent: false,
        print_size: false,
        human_readable: false,
        pattern_glob: None,
        match_dirs: false,
        exclude_patterns: vec![],
        color: false,
        no_color: true,
        ascii: true,
        sort_by_time: false,
        reverse: false,
        print_mod_date: false,
        output_file: None,
        file_limit: None,
        dirs_first: true,
        classify: false,
        no_report: false,
        print_permissions: false,
        from_file: false,
        icons: false,
        prune: false,
    };

    list_directory_as_string(path, &options).map_err(|source| TreeError::ListFailed {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn lists_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("file.txt"), "").unwrap();

        let out = tree(dir.path(), None).unwrap();
        assert!(out.contains("subdir"));
        assert!(out.contains("file.txt"));
    }

    #[test]
    fn depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("top").join("mid").join("deep");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("leaf.txt"), "").unwrap();

        let out = tree(dir.path(), Some(1)).unwrap();
        assert!(out.contains("top"));
        assert!(!out.contains("leaf.txt"));
    }

    #[test]
    fn nonexistent_path() {
        assert!(matches!(
            tree(Path::new("/nonexistent/path/xyz"), None).unwrap_err(),
            TreeError::PathNotFound(_)
        ));
    }

    #[test]
    fn file_path_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "").unwrap();

        assert!(matches!(
            tree(&file, None).unwrap_err(),
            TreeError::NotADirectory(_)
        ));
    }
}
