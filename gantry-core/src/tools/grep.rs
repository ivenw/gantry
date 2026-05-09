use std::fmt;
use std::path::PathBuf;

use gantry_tools::grep::GrepError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;

pub struct GrepTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GrepArgs {
    /// Regex pattern to search for.
    pub pattern: String,
    /// File or directory to search in.
    pub path: PathBuf,
    /// Whether to match case-insensitively. Defaults to false.
    #[serde(default)]
    pub case_insensitive: bool,
    /// Optional glob pattern to restrict which files are searched, e.g. `*.rs`.
    pub glob_filter: Option<String>,
    /// Maximum number of matching lines to return. Defaults to 100.
    pub max_results: Option<usize>,
}

pub struct GrepToolError(pub GrepError);

impl std::error::Error for GrepToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<GrepError> for GrepToolError {
    fn from(e: GrepError) -> Self {
        Self(e)
    }
}

impl fmt::Debug for GrepToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for GrepToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            GrepError::InvalidPattern(msg) => write!(f, "invalid regex pattern: {msg}"),
            GrepError::InvalidGlob(msg) => write!(f, "invalid glob filter: {msg}"),
            GrepError::BuildGlob(msg) => write!(f, "failed to build glob filter: {msg}"),
        }
    }
}

impl Tool for GrepTool {
    const NAME: &'static str = "grep_files";

    type Error = GrepToolError;
    type Args = GrepArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search for a regex pattern in files, recursing into directories. \
                Respects .gitignore and other ignore files. \
                Results are grouped by file and formatted as 'line_num: content'. \
                Returns an empty string if no matches are found."
                .to_string(),
            parameters: schema_for!(GrepArgs).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let pattern = args.pattern.clone();
        let path = args.path.clone();
        let glob_filter = args.glob_filter.clone();
        Ok(tokio::task::spawn_blocking(move || {
            gantry_tools::grep_files(
                &pattern,
                &path,
                args.case_insensitive,
                glob_filter.as_deref(),
                args.max_results,
            )
        })
        .await
        .expect("grep_files task panicked")?)
    }
}
