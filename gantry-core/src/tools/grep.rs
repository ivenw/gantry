use std::path::Path;

use anyhow::{Result, anyhow};
use grep::{
    regex::RegexMatcherBuilder,
    searcher::{BinaryDetection, SearcherBuilder, Sink, SinkMatch},
};
use ignore::{WalkBuilder, overrides::OverrideBuilder};

const DEFAULT_MAX_RESULTS: usize = 100;

/// Searches for `pattern` (a regex) in `path`, recursing into directories.
///
/// Directory traversal respects `.gitignore` and other ignore files.
/// Results are grouped by file and formatted as `{line_num}: {content}`.
///
/// `glob_filter` restricts which files are searched (e.g. `"*.rs"`).
/// `max_results` caps the total number of matching lines returned; defaults
/// to 100. If the cap is hit, a truncation message is appended to the output.
pub fn grep_files(
    pattern: &str,
    path: &Path,
    case_insensitive: bool,
    glob_filter: Option<&str>,
    max_results: Option<usize>,
) -> Result<String> {
    let cap = max_results.unwrap_or(DEFAULT_MAX_RESULTS);

    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(case_insensitive)
        .multi_line(false)
        .build(pattern)
        .map_err(|e| anyhow!("invalid pattern: {e}"))?;

    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .build();

    let mut walk_builder = WalkBuilder::new(path);
    if let Some(glob) = glob_filter {
        let mut overrides = OverrideBuilder::new(path);
        overrides
            .add(glob)
            .map_err(|e| anyhow!("invalid glob filter: {e}"))?;
        let built = overrides
            .build()
            .map_err(|e| anyhow!("failed to build glob filter: {e}"))?;
        walk_builder.overrides(built);
    }

    // file_path -> Vec<(line_number, line_content)>
    let mut grouped: Vec<(String, Vec<(u64, String)>)> = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;

    'outer: for entry in walk_builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        let mut collector = MatchCollector {
            matches: Vec::new(),
            remaining: cap - total,
        };

        searcher
            .search_path(&matcher, entry.path(), &mut collector)
            .ok(); // skip unreadable files silently

        if collector.matches.is_empty() {
            continue;
        }

        let collected = collector.matches.len();
        total += collected;
        grouped.push((entry.path().display().to_string(), collector.matches));

        if total >= cap {
            truncated = true;
            break 'outer;
        }
    }

    Ok(format_results(&grouped, truncated, total, cap))
}

/// A `Sink` implementation that collects `(line_number, line_content)` pairs
/// and stops once `remaining` matches have been found.
struct MatchCollector {
    matches: Vec<(u64, String)>,
    remaining: usize,
}

impl Sink for MatchCollector {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep::searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        let line_num = mat.line_number().unwrap_or(0);
        let content = String::from_utf8_lossy(mat.bytes())
            .trim_end_matches(['\n', '\r'])
            .to_string();
        self.matches.push((line_num, content));
        self.remaining = self.remaining.saturating_sub(1);
        Ok(self.remaining > 0)
    }
}

/// Formats collected match groups into the final output string.
fn format_results(
    grouped: &[(String, Vec<(u64, String)>)],
    truncated: bool,
    total: usize,
    cap: usize,
) -> String {
    if grouped.is_empty() {
        return String::new();
    }

    let width: usize = grouped
        .iter()
        .flat_map(|(_, matches)| matches.iter().map(|(n, _)| *n))
        .map(|n| n.to_string().len())
        .max()
        .unwrap_or(1);

    let mut out = String::new();
    for (i, (file, matches)) in grouped.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(file);
        out.push('\n');
        for (line_num, content) in matches {
            out.push_str(&format!("  {line_num:>width$}: {content}\n"));
        }
    }

    if truncated {
        // Count how many matches were not shown.
        // `total` is capped at `cap`; the actual overflow is unknown, so we
        // just note the cap was hit.
        let _ = total; // used via `cap` message below
        out.push_str(&format!("\n... results truncated at {cap} matches\n"));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup(files: &[(&str, &str)]) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }
        dir
    }

    #[test]
    fn matches_in_single_file() {
        let dir = setup(&[("foo.rs", "fn main() {\n    println!(\"hello\");\n}\n")]);
        let result = grep_files("fn", dir.path(), false, None, None).unwrap();
        assert!(result.contains("foo.rs"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn recurses_into_subdirectory() {
        let dir = setup(&[
            ("a/foo.rs", "fn foo() {}\n"),
            ("b/bar.rs", "fn bar() {}\n"),
        ]);
        let result = grep_files("fn", dir.path(), false, None, None).unwrap();
        assert!(result.contains("fn foo()"));
        assert!(result.contains("fn bar()"));
    }

    #[test]
    fn case_insensitive_matching() {
        let dir = setup(&[("f.txt", "Hello World\nhello world\n")]);
        let sensitive = grep_files("Hello", dir.path(), false, None, None).unwrap();
        let insensitive = grep_files("Hello", dir.path(), true, None, None).unwrap();
        // sensitive: only 1 match
        assert_eq!(sensitive.lines().filter(|l| l.contains("Hello")).count(), 1);
        // insensitive: both lines
        assert_eq!(insensitive.lines().filter(|l| l.contains("ello")).count(), 2);
    }

    #[test]
    fn glob_filter_restricts_file_types() {
        let dir = setup(&[
            ("code.rs", "fn target() {}\n"),
            ("note.txt", "fn target() {}\n"),
        ]);
        let result = grep_files("target", dir.path(), false, Some("*.rs"), None).unwrap();
        assert!(result.contains("code.rs"));
        assert!(!result.contains("note.txt"));
    }

    #[test]
    fn result_cap_with_truncation_message() {
        let content = (1..=20).map(|i| format!("match line {i}\n")).collect::<String>();
        let dir = setup(&[("big.txt", &content)]);
        let result = grep_files("match", dir.path(), false, None, Some(5)).unwrap();
        assert!(result.contains("truncated at 5 matches"));
        let match_lines = result.lines().filter(|l| l.contains("match line")).count();
        assert_eq!(match_lines, 5);
    }

    #[test]
    fn no_matches_returns_empty_string() {
        let dir = setup(&[("f.txt", "nothing here\n")]);
        let result = grep_files("zzznomatch", dir.path(), false, None, None).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn invalid_pattern_returns_error() {
        let dir = setup(&[("f.txt", "content\n")]);
        let err = grep_files("(unclosed", dir.path(), false, None, None);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("invalid pattern"));
    }

    #[test]
    fn output_grouped_by_file() {
        let dir = setup(&[
            ("a.rs", "fn alpha() {}\n"),
            ("b.rs", "fn beta() {}\n"),
        ]);
        let result = grep_files("fn", dir.path(), false, None, None).unwrap();
        let a_pos = result.find("a.rs").unwrap_or(usize::MAX);
        let b_pos = result.find("b.rs").unwrap_or(usize::MAX);
        // Both files present; order may vary but each appears once
        assert_ne!(a_pos, usize::MAX);
        assert_ne!(b_pos, usize::MAX);
    }
}
