use std::path::PathBuf;

use gantry_tools::EditOp;
use gantry_tools::edit::EditError;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use thiserror::Error;

use super::messages;

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

#[derive(Debug, Error)]
pub enum EditToolError {
    #[error("{}", render_edit(.0))]
    Edit(#[from] EditError),
}

fn render_edit(err: &EditError) -> String {
    match err {
        EditError::InvalidLineRef { raw, reason } => messages::edit_invalid_line_ref(raw, reason),
        EditError::StaleReferences(stale) => messages::edit_stale_references(stale),
        EditError::OverlappingEdits { a_start, a_end, b_start, b_end } => {
            messages::edit_overlapping(*a_start, *a_end, *b_start, *b_end)
        }
        EditError::Io(e) => messages::edit_io(e),
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
        let ops = ops.map_err(EditToolError::Edit)?;
        let op_count = ops.len();
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || gantry_tools::edit_file(&path_clone, ops))
            .await
            .expect("edit_file task panicked")
            .map_err(EditToolError::Edit)?;
        Ok(messages::edit_success(&path, op_count))
    }
}
