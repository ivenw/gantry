use std::path::Path;
use std::str::FromStr;

use thiserror::Error;

use crate::hash_line;

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

#[derive(Debug, Error)]
pub enum InvalidLineRefReason {
    #[error("expected 'N#XX' format")]
    MissingHash,
    #[error("invalid line number")]
    InvalidLineNumber,
    #[error("line numbers are 1-indexed, got 0")]
    ZeroLineNumber,
    #[error("hash must be exactly 2 characters")]
    BadHashLength,
}

#[derive(Debug, Clone)]
pub struct StaleLine {
    pub line: usize,
    pub kind: StaleLineKind,
}

#[derive(Debug, Clone)]
pub enum StaleLineKind {
    OutOfRange { file_len: usize },
    HashMismatch { expected: String, actual: String },
}

#[derive(Debug, Error)]
pub enum EditError {
    #[error("invalid line ref {raw:?}: {reason}")]
    InvalidLineRef {
        raw: String,
        reason: InvalidLineRefReason,
    },
    #[error("stale line references")]
    StaleReferences(Vec<StaleLine>),
    #[error("overlapping edits: [{a_start}-{a_end}] and [{b_start}-{b_end}]")]
    OverlappingEdits {
        a_start: usize,
        a_end: usize,
        b_start: usize,
        b_end: usize,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl FromStr for LineRef {
    type Err = EditError;

    /// Parses a line reference of the form `"N#XX"` where `N` is a 1-indexed line number
    /// and `XX` is a 2-character content hash.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (line_part, hash) = s.split_once('#').ok_or_else(|| EditError::InvalidLineRef {
            raw: s.to_string(),
            reason: InvalidLineRefReason::MissingHash,
        })?;
        let line = line_part
            .parse::<usize>()
            .map_err(|_| EditError::InvalidLineRef {
                raw: s.to_string(),
                reason: InvalidLineRefReason::InvalidLineNumber,
            })?;
        if line == 0 {
            return Err(EditError::InvalidLineRef {
                raw: s.to_string(),
                reason: InvalidLineRefReason::ZeroLineNumber,
            });
        }
        if hash.len() != 2 {
            return Err(EditError::InvalidLineRef {
                raw: s.to_string(),
                reason: InvalidLineRefReason::BadHashLength,
            });
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
/// Returns the diff hunks describing what changed.
pub fn edit_file(path: &Path, ops: Vec<EditOp>) -> Result<Vec<DiffHunk>, EditError> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let (result, hunks) = apply_edits(&lines, &ops)?;
    std::fs::write(path, result.join("\n"))?;
    Ok(hunks)
}

// TODO: This we will probably have to change. The current structure allows us to ONLY show diffs
// and we loose all information surrounding the diff which is useful to have in a display context.
// The right shape is to have `old_lines` and `new_lines` be just `String`, so the content of the
// whole file pre and post edit. The hunk compute is then handled by a dedicated diff crate.
/// A single contiguous region that was changed by one edit operation.
///
/// Line numbers are 1-indexed on both sides, matching the unified diff convention.
/// For insert-after ops `old_lines` is empty and `old_start` is the line after which
/// content was inserted. For deletions `new_lines` is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// 1-indexed start line in the original file.
    pub old_start: usize,
    /// 1-indexed start line in the new file.
    pub new_start: usize,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
}

impl DiffHunk {
    pub fn old_count(&self) -> usize {
        self.old_lines.len()
    }

    pub fn new_count(&self) -> usize {
        self.new_lines.len()
    }
}

/// Validates and applies edit operations to a slice of lines.
///
/// Returns the modified lines and the diff hunks describing what changed.
fn apply_edits(lines: &[&str], ops: &[EditOp]) -> Result<(Vec<String>, Vec<DiffHunk>), EditError> {
    validate_hashes(lines, ops)?;

    let mut sorted_ops = ops.to_vec();
    sorted_ops.sort_by(|a, b| b.start.line.cmp(&a.start.line));

    check_overlaps(&sorted_ops)?;

    let mut result: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    // Collected in bottom-up order; new_start is filled in a second pass.
    let mut hunks: Vec<DiffHunk> = Vec::with_capacity(sorted_ops.len());

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

        let old_lines: Vec<String> = if op.end.is_some() {
            lines[start_idx..=end_idx]
                .iter()
                .map(|l| l.to_string())
                .collect()
        } else {
            vec![]
        };

        hunks.push(DiffHunk {
            old_start: op.start.line,
            new_start: 0, // filled below
            old_lines,
            new_lines: new_lines.clone(),
        });

        if op.end.is_none() {
            let insert_at = start_idx + 1;
            for (i, line) in new_lines.into_iter().enumerate() {
                result.insert(insert_at + i, line);
            }
        } else {
            result.splice(start_idx..=end_idx, new_lines);
        }
    }

    // Ops were processed bottom-up; reverse to top-down order to compute new_start.
    hunks.reverse();
    let mut offset: isize = 0;
    for hunk in &mut hunks {
        hunk.new_start = (hunk.old_start as isize + offset) as usize;
        let delta = hunk.new_lines.len() as isize - hunk.old_lines.len() as isize;
        offset += delta;
    }

    Ok((result, hunks))
}

/// Checks that every line reference in `ops` matches the actual content hash at that line.
/// Collects all mismatches and returns them together rather than failing on the first.
fn validate_hashes(lines: &[&str], ops: &[EditOp]) -> Result<(), EditError> {
    let mut stale: Vec<StaleLine> = vec![];

    for op in ops {
        let refs = std::iter::once(&op.start).chain(op.end.as_ref());
        for lref in refs {
            let idx = lref.line - 1;
            if idx >= lines.len() {
                stale.push(StaleLine {
                    line: lref.line,
                    kind: StaleLineKind::OutOfRange {
                        file_len: lines.len(),
                    },
                });
                continue;
            }
            let actual = hash_line(lines[idx]);
            if actual != lref.hash {
                stale.push(StaleLine {
                    line: lref.line,
                    kind: StaleLineKind::HashMismatch {
                        expected: lref.hash.clone(),
                        actual,
                    },
                });
            }
        }
    }

    if stale.is_empty() {
        Ok(())
    } else {
        Err(EditError::StaleReferences(stale))
    }
}

/// Verifies that no two operations in the (descending) sorted list touch overlapping line ranges.
fn check_overlaps(sorted_ops: &[EditOp]) -> Result<(), EditError> {
    // ops are sorted descending by start line
    for i in 0..sorted_ops.len().saturating_sub(1) {
        let a = &sorted_ops[i];
        let b = &sorted_ops[i + 1];
        let a_end = a.end.as_ref().map(|e| e.line).unwrap_or(a.start.line);
        let b_end = b.end.as_ref().map(|e| e.line).unwrap_or(b.start.line);
        // a starts higher, b starts lower; overlap if b's end >= a's start
        if b_end >= a.start.line {
            return Err(EditError::OverlappingEdits {
                a_start: a.start.line,
                a_end,
                b_start: b.start.line,
                b_end,
            });
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
        let (result, _) = apply_edits(&src, &ops).unwrap();
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
        let (result, _) = apply_edits(&src, &ops).unwrap();
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
        let (result, _) = apply_edits(&src, &ops).unwrap();
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
        let (result, _) = apply_edits(&src, &ops).unwrap();
        assert_eq!(result, vec!["a", "d"]);
    }

    #[test]
    fn batch_top_to_bottom_order_applied_correctly() {
        let src = lines("a\nb\nc\nd\ne");
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
        let (result, _) = apply_edits(&src, &ops).unwrap();
        assert_eq!(result, vec!["a", "B", "c", "D", "e"]);
    }

    #[test]
    fn staleness_rejection() {
        let src = lines("a\nb\nc");
        let ops = vec![EditOp {
            start: LineRef {
                line: 2,
                hash: "xx".into(),
            },
            end: Some(ref_of(2, "b")),
            content: Some("Z".into()),
        }];
        assert!(matches!(
            apply_edits(&src, &ops).unwrap_err(),
            EditError::StaleReferences(_)
        ));
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
                },
                end: Some(ref_of(2, "b")),
                content: Some("B".into()),
            },
        ];
        assert!(matches!(
            apply_edits(&src, &ops).unwrap_err(),
            EditError::StaleReferences(_)
        ));
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
        assert!(matches!(
            apply_edits(&src, &ops).unwrap_err(),
            EditError::OverlappingEdits { .. }
        ));
    }

    #[test]
    fn line_ref_parse_valid() {
        let r: LineRef = "5#a7".parse().unwrap();
        assert_eq!(r.line, 5);
        assert_eq!(r.hash, "a7");
    }

    #[test]
    fn line_ref_parse_invalid() {
        assert!(matches!(
            "abc".parse::<LineRef>().unwrap_err(),
            EditError::InvalidLineRef {
                reason: InvalidLineRefReason::MissingHash,
                ..
            }
        ));
        assert!(matches!(
            "0#ab".parse::<LineRef>().unwrap_err(),
            EditError::InvalidLineRef {
                reason: InvalidLineRefReason::ZeroLineNumber,
                ..
            }
        ));
        assert!(matches!(
            "5#a".parse::<LineRef>().unwrap_err(),
            EditError::InvalidLineRef {
                reason: InvalidLineRefReason::BadHashLength,
                ..
            }
        ));
        assert!(matches!(
            "5#abc".parse::<LineRef>().unwrap_err(),
            EditError::InvalidLineRef {
                reason: InvalidLineRefReason::BadHashLength,
                ..
            }
        ));
    }
}
