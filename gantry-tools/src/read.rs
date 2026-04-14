use std::path::Path;

use thiserror::Error;

use crate::hash_line;

#[derive(Debug, Error)]
pub enum ReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Reads a file from disk and returns its contents formatted with line numbers and hashes.
///
/// Each line is prefixed with `{line_num}#{hash}| ` where `hash` is a 2-character content hash
/// used to detect staleness in subsequent edit operations.
///
/// `offset` is 1-indexed; if provided, reading starts at that line number.
/// `limit` caps the number of lines returned.
pub fn read_file(path: &Path, offset: Option<usize>, limit: Option<usize>) -> Result<String, ReadError> {
    let content = std::fs::read_to_string(path)?;
    Ok(format_hashlines(&content, offset, limit))
}

/// Formats file content as hash-annotated lines, optionally sliced by offset and limit.
fn format_hashlines(content: &str, offset: Option<usize>, limit: Option<usize>) -> String {
    let lines: Vec<&str> = content.lines().collect();

    let start = offset.map(|o| o.saturating_sub(1)).unwrap_or(0);
    let end = limit
        .map(|l| (start + l).min(lines.len()))
        .unwrap_or(lines.len());
    let slice = &lines[start.min(lines.len())..end];

    if slice.is_empty() {
        return String::new();
    }

    let max_line_num = start + slice.len();
    let width = max_line_num.to_string().len();

    slice
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let line_num = start + i + 1;
            let hash = hash_line(line);
            format!("{line_num:>width$}#{hash}| {line}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_line;

    const SAMPLE: &str = "fn main() {\n    println!(\"hello\");\n}";

    #[test]
    fn full_file_format() {
        let out = format_hashlines(SAMPLE, None, None);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("1#"));
        assert!(lines[1].starts_with("2#"));
        assert!(lines[2].starts_with("3#"));
    }

    #[test]
    fn hash_matches_content() {
        let out = format_hashlines(SAMPLE, None, None);
        let first_line = out.lines().next().unwrap();
        // format: "1#XY| fn main() {"
        let hash_part = &first_line[2..4];
        let expected = hash_line("fn main() {");
        assert_eq!(hash_part, expected);
    }

    #[test]
    fn offset_and_limit() {
        let content = "a\nb\nc\nd\ne";
        let out = format_hashlines(content, Some(2), Some(2));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        // line numbers should be 2 and 3
        assert!(lines[0].starts_with("2#"));
        assert!(lines[1].starts_with("3#"));
        // content should be b and c
        assert!(lines[0].ends_with("| b"));
        assert!(lines[1].ends_with("| c"));
    }

    #[test]
    fn line_number_right_alignment() {
        let content = (1..=10)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = format_hashlines(&content, None, None);
        let first_line = out.lines().next().unwrap();
        assert!(
            first_line.starts_with(" 1#"),
            "expected ' 1#...', got: {first_line}"
        );
        let last_line = out.lines().last().unwrap();
        assert!(
            last_line.starts_with("10#"),
            "expected '10#...', got: {last_line}"
        );
    }

    #[test]
    fn empty_file() {
        assert_eq!(format_hashlines("", None, None), "");
    }

    #[test]
    fn single_line_no_trailing_newline() {
        let out = format_hashlines("hello", None, None);
        assert_eq!(out.lines().count(), 1);
        assert!(out.starts_with("1#"));
        assert!(out.ends_with("| hello"));
    }

    #[test]
    fn trailing_newline_not_extra_line() {
        let with_newline = format_hashlines("a\nb\n", None, None);
        let without_newline = format_hashlines("a\nb", None, None);
        assert_eq!(with_newline.lines().count(), without_newline.lines().count());
    }
}
