use std::path::PathBuf;

use gantry_tools::tree::TreeError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use thiserror::Error;

use super::boundary::BoundaryError;
use super::messages;

pub struct TreeTool;

#[derive(Debug, Deserialize)]
pub struct TreeArgs {
    pub path: PathBuf,
    pub depth: Option<u32>,
}

#[derive(Debug, Error)]
pub enum TreeToolError {
    #[error("{}", render_tree(.0))]
    Tree(#[from] TreeError),
    #[error(transparent)]
    Boundary(#[from] BoundaryError),
}

fn render_tree(err: &TreeError) -> String {
    match err {
        TreeError::PathNotFound(path) => messages::tree_path_not_found(path),
        TreeError::NotADirectory(path) => messages::tree_not_a_directory(path),
        TreeError::ListFailed { path, source } => messages::tree_list_failed(path, source),
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
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory to list."
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Maximum recursion depth. Omit for unlimited depth."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = args.path.clone();
        Ok(
            tokio::task::spawn_blocking(move || gantry_tools::tree(&path, args.depth))
                .await
                .map_err(BoundaryError::from)??,
        )
    }
}
