use std::path::PathBuf;

use gantry_tools::grep::GrepError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use thiserror::Error;

use super::messages;

pub struct GrepTool;

#[derive(Debug, Deserialize)]
pub struct GrepArgs {
    pub pattern: String,
    pub path: PathBuf,
    #[serde(default)]
    pub case_insensitive: bool,
    pub glob_filter: Option<String>,
    pub max_results: Option<usize>,
}

#[derive(Debug, Error)]
pub enum GrepToolError {
    #[error("{}", render_grep(.0))]
    Grep(#[from] GrepError),
}

fn render_grep(err: &GrepError) -> String {
    match err {
        GrepError::InvalidPattern(msg) => messages::grep_invalid_pattern(msg),
        GrepError::InvalidGlob(msg) => messages::grep_invalid_glob(msg),
        GrepError::BuildGlob(msg) => messages::grep_build_glob(msg),
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
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for."
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in."
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "Whether to match case-insensitively. Defaults to false."
                    },
                    "glob_filter": {
                        "type": "string",
                        "description": "Optional glob pattern to restrict which files are searched, e.g. '*.rs'."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of matching lines to return. Defaults to 100."
                    }
                },
                "required": ["pattern", "path"]
            }),
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
