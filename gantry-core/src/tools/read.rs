use std::fmt;
use std::path::PathBuf;

use gantry_tools::read::ReadError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;

pub struct ReadTool;

#[derive(Debug, Deserialize)]
pub struct ReadArgs {
    pub path: PathBuf,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

pub struct ReadToolError(pub ReadError);

impl std::error::Error for ReadToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<ReadError> for ReadToolError {
    fn from(e: ReadError) -> Self {
        Self(e)
    }
}

impl fmt::Debug for ReadToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for ReadToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ReadError::Io(e) => write!(f, "failed to read file: {e}"),
        }
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
            description:
                "Read a file and return its contents with line numbers and content hashes. \
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
        Ok(tokio::task::spawn_blocking(move || {
            gantry_tools::read_file(&path, args.offset, args.limit)
        })
        .await
        .expect("read_file task panicked")?)
    }
}
