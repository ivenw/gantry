use std::path::PathBuf;

use gantry_tools::read::ReadError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use thiserror::Error;

use super::boundary::BoundaryError;
use super::messages;

pub struct ReadTool;

#[derive(Debug, Deserialize)]
pub struct ReadArgs {
    pub path: PathBuf,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Error)]
pub enum ReadToolError {
    #[error("{}", render_read(.0))]
    Read(#[from] ReadError),
    #[error(transparent)]
    Boundary(#[from] BoundaryError),
}

fn render_read(err: &ReadError) -> String {
    match err {
        ReadError::Io(e) => messages::read_io(e),
    }
}

impl Tool for ReadTool {
    const NAME: &'static str = "read_file";

    type Error = ReadToolError;
    type Args = ReadArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read a file and return its contents with line numbers and content hashes. \
                Each line is prefixed with 'N#XX| ' where N is the 1-indexed line number and XX is \
                a 2-character hash used to identify lines for subsequent edits."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "1-indexed line number to start reading from. Defaults to the beginning of the file."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to return."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = args.path.clone();
        Ok(
            tokio::task::spawn_blocking(move || {
                gantry_tools::read_file(&path, args.offset, args.limit)
            })
            .await
            .map_err(BoundaryError::from)??,
        )
    }
}
