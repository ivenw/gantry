use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};

use super::hash_line;

#[derive(Debug, Clone)]
pub struct EditOp {
    pub start: LineRef,
    pub end: Option<LineRef>,
    pub content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LineRef {
    pub line: usize,
    pub hash: String,
}

impl FromStr for LineRef {
    type Err = anyhow::Error;

    /// Parses a line reference of the form `"N#XX"` where `N` is a 1-indexed line number
    /// and `XX` is a 2-character content hash.
    fn from_str(s: &str) -> Result<Self> {
        let (line_part, hash) = s
            .split_once('#')
            .ok_or_else(|| anyhow!("invalid line ref {s:?}: expected 'N#XX' format"))?;
        let line = line_part
            .parse::<usize>()
            .map_err(|_| anyhow!("invalid line number in ref {s:?}"))?;
        if line == 0 {
            bail!("line numbers are 1-indexed, got 0 in ref {s:?}");
        }
        if hash.len() != 2 {
            bail!("hash in ref {s:?} must be exactly 2 characters");
        }
        Ok(Self {
            line,
            hash: hash.to_string(),
        })
    }
}

/// Applies a batch of edit operations to a file in place.
///
/// All line references are validated against their hashes before any edits are applied.
/// If any reference is stale, the entire batch is rejected. Overlapping ranges are also rejected.
/// Operations are applied bottom-up so earlier line numbers remain valid throughout.
pub fn edit_file(path: &Path, ops: Vec<EditOp>) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let result = apply_edits(&lines, &ops)?;
    std::fs::write(path, result.join("\n"))?;
    Ok(())
}

/// Validates and applies edit operations to a slice of lines, returning the modified lines.
fn apply_edits(lines: &[&str], ops: &[EditOp]) -> Result<Vec<String>> {
    validate_hashes(lines, ops)?;

    let mut sorted_ops = ops.to_vec();
    sorted_ops.sort_by(|a, b| b.start.line.cmp(&a.start.line));

    check_overlaps(&sorted_ops)?;

    let mut result: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    for op in &sorted_ops {
        let start_idx = op.start.line - 1;
        let end_idx = op.end.as_ref().map(|e| e.line - 1).unwrap_or(start_idx);

        let new_lines: Vec<String> = match &op.content {
            Some(c) if !c.is_empty() => c.lines().map(|l| l.to_string()).collect(),
            _ if op.end.is_none() => {
                // insert-after with no content is a no-op
                continue;
            }
            _ => vec![],
        };

        if op.end.is_none() {
            // insert after start_idx
            let insert_at = start_idx + 1;
            for (i, line) in new_lines.into_iter().enumerate() {
                result.insert(insert_at + i, line);
            }
        } else {
            result.splice(start_idx..=end_idx, new_lines);
        }
    }

    Ok(result)
}

/// Checks that every line reference in `ops` matches the actual content hash at that line.
/// Collects all mismatches and returns them together rather than failing on the first.
fn validate_hashes(lines: &[&str], ops: &[EditOp]) -> Result<()> {
    let mut stale: Vec<String> = vec![];

    for op in ops {
        let refs = std::iter::once(&op.start).chain(op.end.as_ref());
        for lref in refs {
            let idx = lref.line - 1;
            if idx >= lines.len() {
                stale.push(format!(
                    "line {} does not exist (file has {} lines)",
                    lref.line,
                    lines.len()
                ));
                continue;
            }
            let actual = hash_line(lines[idx]);
            if actual != lref.hash {
                stale.push(format!(
                    "line {} is stale: expected hash '{}', got '{}'",
                    lref.line, lref.hash, actual
                ));
            }
        }
    }

    if stale.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("stale line references:\n{}", stale.join("\n")))
    }
}

/// Verifies that no two operations in the (descending) sorted list touch overlapping line ranges.
fn check_overlaps(sorted_ops: &[EditOp]) -> Result<()> {
    // ops are sorted descending by start line
    for i in 0..sorted_ops.len().saturating_sub(1) {
        let a = &sorted_ops[i];
        let b = &sorted_ops[i + 1];
        let a_end = a.end.as_ref().map(|e| e.line).unwrap_or(a.start.line);
        let b_end = b.end.as_ref().map(|e| e.line).unwrap_or(b.start.line);
        // a starts higher, b starts lower; overlap if b's end >= a's start
        if b_end >= a.start.line {
            bail!(
                "overlapping edits: [{}-{}] and [{}-{}]",
                b.start.line,
                b_end,
                a.start.line,
                a_end
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    fn hash(s: &str) -> String {
        hash_line(s)
    }

    fn ref_of(line: usize, content: &str) -> LineRef {
        LineRef {
            line,
            hash: hash(content),
        }
    }

    #[test]
    fn replace_range() {
        let src = lines("a\nb\nc\nd");
        let ops = vec![EditOp {
            start: ref_of(2, "b"),
            end: Some(ref_of(3, "c")),
            content: Some("X\nY".into()),
        }];
        let result = apply_edits(&src, &ops).unwrap();
        assert_eq!(result, vec!["a", "X", "Y", "d"]);
    }

    #[test]
    fn replace_single_line() {
        let src = lines("a\nb\nc");
        let ops = vec![EditOp {
            start: ref_of(2, "b"),
            end: Some(ref_of(2, "b")),
            content: Some("Z".into()),
        }];
        let result = apply_edits(&src, &ops).unwrap();
        assert_eq!(result, vec!["a", "Z", "c"]);
    }

    #[test]
    fn insert_after() {
        let src = lines("a\nb\nc");
        let ops = vec![EditOp {
            start: ref_of(2, "b"),
            end: None,
            content: Some("X\nY".into()),
        }];
        let result = apply_edits(&src, &ops).unwrap();
        assert_eq!(result, vec!["a", "b", "X", "Y", "c"]);
    }

    #[test]
    fn delete_range() {
        let src = lines("a\nb\nc\nd");
        let ops = vec![EditOp {
            start: ref_of(2, "b"),
            end: Some(ref_of(3, "c")),
            content: None,
        }];
        let result = apply_edits(&src, &ops).unwrap();
        assert_eq!(result, vec!["a", "d"]);
    }

    #[test]
    fn batch_top_to_bottom_order_applied_correctly() {
        let src = lines("a\nb\nc\nd\ne");
        // both ops specified top-to-bottom; tool must reorder
        let ops = vec![
            EditOp {
                start: ref_of(2, "b"),
                end: Some(ref_of(2, "b")),
                content: Some("B".into()),
            },
            EditOp {
                start: ref_of(4, "d"),
                end: Some(ref_of(4, "d")),
                content: Some("D".into()),
            },
        ];
        let result = apply_edits(&src, &ops).unwrap();
        assert_eq!(result, vec!["a", "B", "c", "D", "e"]);
    }

    #[test]
    fn staleness_rejection() {
        let src = lines("a\nb\nc");
        let ops = vec![EditOp {
            start: LineRef {
                line: 2,
                hash: "xx".into(),
            }, // wrong hash
            end: Some(ref_of(2, "b")),
            content: Some("Z".into()),
        }];
        assert!(apply_edits(&src, &ops).is_err());
    }

    #[test]
    fn partial_staleness_rejects_entire_batch() {
        let src = lines("a\nb\nc");
        let ops = vec![
            EditOp {
                start: ref_of(1, "a"),
                end: Some(ref_of(1, "a")),
                content: Some("A".into()),
            },
            EditOp {
                start: LineRef {
                    line: 2,
                    hash: "xx".into(),
                }, // stale
                end: Some(ref_of(2, "b")),
                content: Some("B".into()),
            },
        ];
        assert!(apply_edits(&src, &ops).is_err());
    }

    #[test]
    fn overlapping_ranges_error() {
        let src = lines("a\nb\nc\nd");
        let ops = vec![
            EditOp {
                start: ref_of(1, "a"),
                end: Some(ref_of(3, "c")),
                content: Some("X".into()),
            },
            EditOp {
                start: ref_of(2, "b"),
                end: Some(ref_of(2, "b")),
                content: Some("Y".into()),
            },
        ];
        assert!(apply_edits(&src, &ops).is_err());
    }

    #[test]
    fn line_ref_parse_valid() {
        let r: LineRef = "5#a7".parse().unwrap();
        assert_eq!(r.line, 5);
        assert_eq!(r.hash, "a7");
    }

    #[test]
    fn line_ref_parse_invalid() {
        assert!("abc".parse::<LineRef>().is_err());
        assert!("0#ab".parse::<LineRef>().is_err());
        assert!("5#a".parse::<LineRef>().is_err()); // hash too short
        assert!("5#abc".parse::<LineRef>().is_err()); // hash too long
    }
}
