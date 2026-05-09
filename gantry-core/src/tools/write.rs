use std::fmt;
use std::path::PathBuf;

use gantry_tools::write::WriteError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;

pub struct WriteTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteArgs {
    /// Path of the file to create.
    pub path: PathBuf,
    /// Content to write. An empty string creates a zero-byte file.
    pub content: String,
}

pub struct WriteToolError(pub WriteError);

impl std::error::Error for WriteToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<WriteError> for WriteToolError {
    fn from(e: WriteError) -> Self {
        Self(e)
    }
}

impl fmt::Debug for WriteToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for WriteToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            WriteError::FileExists(path) => write!(
                f,
                "file already exists: {}; use the edit tool to modify existing files",
                path.display()
            ),
            WriteError::Io(e) => write!(f, "I/O error while writing file: {e}"),
        }
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
            parameters: schema_for!(WriteArgs).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let byte_count = args.content.len();
        let path = args.path.clone();
        let content = args.content.clone();
        tokio::task::spawn_blocking(move || gantry_tools::write_file(&path, &content))
            .await
            .expect("write_file task panicked")
            .map_err(WriteToolError::from)?;
        Ok(format!("wrote {byte_count} bytes to {}", args.path.display()))
    }
}
