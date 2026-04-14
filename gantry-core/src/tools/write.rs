use std::path::PathBuf;

use gantry_tools::write::WriteError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use thiserror::Error;

use super::boundary::BoundaryError;
use super::messages;

pub struct WriteTool;

#[derive(Debug, Deserialize)]
pub struct WriteArgs {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Error)]
pub enum WriteToolError {
    #[error("{}", render_write(.0))]
    Write(#[from] WriteError),
    #[error(transparent)]
    Boundary(#[from] BoundaryError),
}

fn render_write(err: &WriteError) -> String {
    match err {
        WriteError::FileExists(path) => messages::write_file_exists(path),
        WriteError::Io(e) => messages::write_io(e),
    }
}

impl Tool for WriteTool {
    const NAME: &'static str = "write_file";

    type Error = WriteToolError;
    type Args = WriteArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Create a new file with the given content. \
                Fails if the file already exists — use the edit tool to modify existing files. \
                Intermediate directories are created automatically."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path of the file to create."
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write. An empty string creates a zero-byte file."
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = args.path.clone();
        let byte_count = args.content.len();
        let content = args.content.clone();
        tokio::task::spawn_blocking(move || gantry_tools::write_file(&path, &content))
            .await
            .map_err(BoundaryError::from)??;
        Ok(messages::write_success(&args.path, byte_count))
    }
}
