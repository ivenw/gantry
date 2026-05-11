pub mod bash;
pub mod edit;
pub mod grep;
pub mod read;
pub mod tree;
pub mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use tree::TreeTool;
pub use write::WriteTool;

use std::path::{Path, PathBuf};

/// Resolves `path` against `cwd`: returns `path` unchanged if absolute, otherwise joins it with `cwd`.
pub(crate) fn resolve_path(cwd: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}
