use std::path::Path;

use gantry_tools::edit::{InvalidLineRefReason, StaleLine, StaleLineKind};

// ── edit ─────────────────────────────────────────────────────────────────────

pub fn edit_success(path: &Path, op_count: usize) -> String {
    format!("applied {op_count} edit(s) to {}", path.display())
}

pub fn edit_invalid_line_ref(raw: &str, reason: &InvalidLineRefReason) -> String {
    format!("invalid line ref {raw:?}: {reason}")
}

pub fn edit_stale_references(stale: &[StaleLine]) -> String {
    let lines: Vec<String> = stale
        .iter()
        .map(|s| match &s.kind {
            StaleLineKind::OutOfRange { file_len } => {
                format!(
                    "line {} does not exist (file has {} lines)",
                    s.line, file_len
                )
            }
            StaleLineKind::HashMismatch { expected, actual } => {
                format!(
                    "line {} is stale: expected hash '{expected}', got '{actual}'",
                    s.line
                )
            }
        })
        .collect();
    format!("stale line references:\n{}", lines.join("\n"))
}

pub fn edit_overlapping(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> String {
    format!("overlapping edits: [{b_start}-{b_end}] and [{a_start}-{a_end}]")
}

pub fn edit_io(err: &std::io::Error) -> String {
    format!("I/O error while editing file: {err}")
}

// ── read ─────────────────────────────────────────────────────────────────────

pub fn read_io(err: &std::io::Error) -> String {
    format!("failed to read file: {err}")
}

// ── grep ─────────────────────────────────────────────────────────────────────

pub fn grep_invalid_pattern(msg: &str) -> String {
    format!("invalid regex pattern: {msg}")
}

pub fn grep_invalid_glob(msg: &str) -> String {
    format!("invalid glob filter: {msg}")
}

pub fn grep_build_glob(msg: &str) -> String {
    format!("failed to build glob filter: {msg}")
}

// ── write ────────────────────────────────────────────────────────────────────

pub fn write_success(path: &Path, byte_count: usize) -> String {
    format!("wrote {byte_count} bytes to {}", path.display())
}

pub fn write_file_exists(path: &Path) -> String {
    format!(
        "file already exists: {}; use the edit tool to modify existing files",
        path.display()
    )
}

pub fn write_io(err: &std::io::Error) -> String {
    format!("I/O error while writing file: {err}")
}

// ── tree ─────────────────────────────────────────────────────────────────────

pub fn tree_path_not_found(path: &Path) -> String {
    format!("path does not exist: {}", path.display())
}

pub fn tree_not_a_directory(path: &Path) -> String {
    format!("path is not a directory: {}", path.display())
}

pub fn tree_list_failed(path: &Path, err: &std::io::Error) -> String {
    format!("failed to list directory {}: {err}", path.display())
}

