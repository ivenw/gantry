use std::path::Path;

use anyhow::{Context, Result, bail};
use rust_tree::rust_tree::{options::TreeOptions, traversal::list_directory_as_string};

/// Lists a directory tree as a string.
///
/// `depth` limits how many levels deep to recurse (`None` = unlimited).
pub fn tree(path: &Path, depth: Option<u32>) -> Result<String> {
    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }
    if !path.is_dir() {
        bail!("path is not a directory: {}", path.display());
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

    list_directory_as_string(path, &options)
        .context(format!("failed to list directory: {}", path.display()))
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
        let err = tree(Path::new("/nonexistent/path/xyz"), None).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn file_path_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "").unwrap();

        let err = tree(&file, None).unwrap_err();
        assert!(err.to_string().contains("not a directory"));
    }
}
