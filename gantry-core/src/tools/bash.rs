use std::fmt;

use gantry_tools::bash::BashError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;

pub struct BashTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BashArgs {
    /// The shell command to execute.
    pub command: String,
    /// Maximum execution time in milliseconds. Defaults to 30 000.
    pub timeout_ms: Option<u64>,
}

pub struct BashToolError(pub BashError);

impl std::error::Error for BashToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<BashError> for BashToolError {
    fn from(e: BashError) -> Self {
        Self(e)
    }
}

impl fmt::Debug for BashToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for BashToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            BashError::Spawn(e) => write!(f, "failed to spawn process: {e}"),
            BashError::Timeout(ms) => write!(f, "command timed out after {ms}ms"),
        }
    }
}

impl Tool for BashTool {
    const NAME: &'static str = "bash";

    type Error = BashToolError;
    type Args = BashArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute a bash command and return its stdout and stderr output combined."
                .to_string(),
            parameters: schema_for!(BashArgs).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let command = args.command.clone();
        Ok(tokio::task::spawn_blocking(move || {
            gantry_tools::run_bash(&command, args.timeout_ms)
        })
        .await
        .expect("bash task panicked")?)
    }
}
