use std::fmt;
use std::path::PathBuf;

use gantry_tools::tree::TreeError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;

pub struct TreeTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TreeArgs {
    /// Path to the directory to list.
    pub path: PathBuf,
    /// Maximum recursion depth. Omit for unlimited depth.
    pub depth: Option<u32>,
}

pub struct TreeToolError(pub TreeError);

impl std::error::Error for TreeToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<TreeError> for TreeToolError {
    fn from(e: TreeError) -> Self {
        Self(e)
    }
}

impl fmt::Debug for TreeToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for TreeToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TreeError::PathNotFound(path) => {
                write!(f, "path does not exist: {}", path.display())
            }
            TreeError::NotADirectory(path) => {
                write!(f, "path is not a directory: {}", path.display())
            }
            TreeError::ListFailed { path, source } => {
                write!(f, "failed to list directory {}: {source}", path.display())
            }
        }
    }
}

impl Tool for TreeTool {
    const NAME: &'static str = "tree";

    type Error = TreeToolError;
    type Args = TreeArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List a directory tree as a formatted string.".to_string(),
            parameters: schema_for!(TreeArgs).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = args.path.clone();
        Ok(
            tokio::task::spawn_blocking(move || gantry_tools::tree(&path, args.depth))
                .await
                .expect("tree task panicked")?,
        )
    }
}
