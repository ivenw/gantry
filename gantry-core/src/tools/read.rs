use std::fmt;
use std::path::PathBuf;

use gantry_tools::read::ReadError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;

use super::resolve_path;

pub struct ReadTool {
    pub cwd: PathBuf,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadArgs {
    /// Path to the file to read.
    pub path: PathBuf,
    /// 1-indexed line number to start reading from. Defaults to the beginning of the file.
    pub offset: Option<usize>,
    /// Maximum number of lines to return.
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
        f.write_str(super::TOOL_ERROR_PREFIX)?;
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
            parameters: schema_for!(ReadArgs).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_path(&self.cwd, args.path);
        Ok(tokio::task::spawn_blocking(move || {
            gantry_tools::read_file(&path, args.offset, args.limit)
        })
        .await
        .expect("read_file task panicked")?)
    }
}
