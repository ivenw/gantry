use std::fmt;
use std::path::PathBuf;

use gantry_tools::EditOp;
use gantry_tools::edit::{EditError, InvalidLineRefReason, StaleLine, StaleLineKind};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;

pub struct EditTool;

/// DTO for a single edit operation. Line references are passed as strings in
/// 'N#XX' format and parsed into `gantry_tools::LineRef` inside `call`.
#[derive(Debug, Deserialize)]
pub struct EditOpDto {
    pub start: String,
    pub end: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EditArgsDto {
    pub path: PathBuf,
    pub ops: Vec<EditOpDto>,
}

pub struct EditToolError(pub EditError);

impl std::error::Error for EditToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<EditError> for EditToolError {
    fn from(e: EditError) -> Self {
        Self(e)
    }
}

impl fmt::Debug for EditToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for EditToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            EditError::InvalidLineRef { raw, reason } => {
                let detail = match reason {
                    InvalidLineRefReason::MissingHash => "expected 'N#XX' format",
                    InvalidLineRefReason::InvalidLineNumber => "invalid line number",
                    InvalidLineRefReason::ZeroLineNumber => "line numbers are 1-indexed, got 0",
                    InvalidLineRefReason::BadHashLength => "hash must be exactly 2 characters",
                };
                write!(f, "invalid line ref {raw:?}: {detail}")
            }
            EditError::StaleReferences(stale) => {
                write!(f, "stale line references:")?;
                for s in stale {
                    write!(f, "\n{}", fmt_stale_line(s))?;
                }
                Ok(())
            }
            EditError::OverlappingEdits { a_start, a_end, b_start, b_end } => {
                write!(f, "overlapping edits: [{b_start}-{b_end}] and [{a_start}-{a_end}]")
            }
            EditError::Io(e) => write!(f, "I/O error while editing file: {e}"),
        }
    }
}

fn fmt_stale_line(s: &StaleLine) -> String {
    match &s.kind {
        StaleLineKind::OutOfRange { file_len } => {
            format!("line {} does not exist (file has {} lines)", s.line, file_len)
        }
        StaleLineKind::HashMismatch { expected, actual } => {
            format!("line {} is stale: expected hash '{expected}', got '{actual}'", s.line)
        }
    }
}

impl Tool for EditTool {
    const NAME: &'static str = "edit_file";

    type Error = EditToolError;
    type Args = EditArgsDto;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Apply a batch of edit operations to an existing file. \
                All line references are validated against their content hashes before any \
                changes are written. The entire batch is rejected if any reference is stale \
                or ranges overlap. Use read_file first to obtain current line numbers and hashes."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to edit."
                    },
                    "ops": {
                        "type": "array",
                        "description": "List of edit operations to apply.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "start": {
                                    "type": "string",
                                    "description": "Start line reference in 'N#XX' format (1-indexed line number and 2-char hash)."
                                },
                                "end": {
                                    "type": "string",
                                    "description": "Optional end line reference. If omitted, the operation inserts after start."
                                },
                                "content": {
                                    "type": "string",
                                    "description": "Replacement content. If omitted with an end ref, the range is deleted."
                                }
                            },
                            "required": ["start"]
                        }
                    }
                },
                "required": ["path", "ops"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = args.path.clone();
        let ops: Result<Vec<EditOp>, EditError> = args
            .ops
            .into_iter()
            .map(|dto| {
                Ok::<EditOp, EditError>(EditOp {
                    start: dto.start.parse()?,
                    end: dto.end.map(|s| s.parse()).transpose()?,
                    content: dto.content,
                })
            })
            .collect();
        let ops = ops.map_err(EditToolError::from)?;
        let op_count = ops.len();
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || gantry_tools::edit_file(&path_clone, ops))
            .await
            .expect("edit_file task panicked")
            .map_err(EditToolError::from)?;
        Ok(format!("applied {op_count} edit(s) to {}", path.display()))
    }
}
