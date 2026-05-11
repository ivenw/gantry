/// Structured input tokens for composing user messages with inline attachments.
///
/// A user's raw input is decomposed into a sequence of [`InputToken`]s before being
/// sent. Plain text regions become [`InputToken::Text`]; resolved file/directory
/// references become [`InputToken::Path`]; skill references become
/// [`InputToken::Skill`]. [`build_user_message`] expands the tokens into a
/// [`Message`] with attachment contents appended as XML blocks.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gantry_tools::{read_file, tree};

use crate::message::Message;

/// A single token in a structured user input.
#[derive(Debug, Clone, PartialEq)]
pub enum InputToken {
    /// Plain text written by the user.
    Text(String),
    /// An absolute path to a file or directory, resolved before sending.
    Path(PathBuf),
    /// A resolved skill: display name and absolute path to its `SKILL.md`.
    Skill { name: String, path: PathBuf },
}

/// Builds a [`Message`] from a sequence of [`InputToken`]s, eagerly reading all
/// attachments from disk.
///
/// Text tokens are concatenated to form the message body. Path and skill tokens are
/// read from disk and appended as XML `<attachment>` blocks after the body. Paths
/// must be absolute; `project_root` is used only to make paths relative in the XML
/// `path` attribute.
pub async fn build_user_message(tokens: Vec<InputToken>, project_root: &Path) -> Result<Message> {
    let mut body = String::new();
    let mut attachments = String::new();

    for token in tokens {
        match token {
            InputToken::Text(text) => body.push_str(&text),
            InputToken::Path(path) => {
                let display = path
                    .strip_prefix(project_root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                let xml = expand_path(&path, &display)
                    .with_context(|| format!("failed to read attachment: {}", display))?;
                attachments.push('\n');
                attachments.push_str(&xml);
            }
            InputToken::Skill { name, path } => {
                let xml = expand_skill(&name, &path)
                    .with_context(|| format!("failed to read skill: {}", name))?;
                attachments.push('\n');
                attachments.push_str(&xml);
            }
        }
    }

    let content = if attachments.is_empty() {
        body
    } else {
        format!("{}{}", body, attachments)
    };

    Ok(Message::user(content))
}

/// Reads a path (file or directory) and wraps the contents in an XML attachment block.
fn expand_path(path: &Path, display: &str) -> Result<String> {
    if path.is_dir() {
        let contents = tree(path, None).with_context(|| format!("tree failed for {}", display))?;
        Ok(format!(
            "<attachment type=\"dir\" path=\"{}\">\n{}\n</attachment>",
            display, contents
        ))
    } else {
        let contents =
            read_file(path, None, None).with_context(|| format!("read failed for {}", display))?;
        Ok(format!(
            "<attachment type=\"file\" path=\"{}\">\n{}\n</attachment>",
            display, contents
        ))
    }
}

/// Reads a skill's `SKILL.md` and wraps the contents in an XML attachment block.
fn expand_skill(name: &str, path: &Path) -> Result<String> {
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("read failed for {}", name))?;
    Ok(format!(
        "<attachment type=\"skill\" name=\"{}\">\n{}\n</attachment>",
        name, contents
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn text_only_tokens_produce_plain_message() {
        let dir = TempDir::new().unwrap();
        let tokens = vec![
            InputToken::Text("hello ".to_string()),
            InputToken::Text("world".to_string()),
        ];
        let msg = build_user_message(tokens, dir.path()).await.unwrap();
        assert_eq!(msg.text(), "hello world");
    }

    #[tokio::test]
    async fn file_token_appends_xml_block() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("foo.txt");
        std::fs::File::create(&file_path)
            .unwrap()
            .write_all(b"line one\nline two\n")
            .unwrap();
        let tokens = vec![
            InputToken::Text("check this: ".to_string()),
            InputToken::Path(file_path),
        ];
        let msg = build_user_message(tokens, dir.path()).await.unwrap();
        let text = msg.text();
        assert!(text.starts_with("check this: "));
        assert!(text.contains("<attachment type=\"file\" path=\"foo.txt\">"));
        assert!(text.contains("</attachment>"));
    }

    #[tokio::test]
    async fn dir_token_appends_xml_block() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("subdir");
        std::fs::create_dir(&sub).unwrap();
        let tokens = vec![InputToken::Path(sub)];
        let msg = build_user_message(tokens, dir.path()).await.unwrap();
        let text = msg.text();
        assert!(text.contains("<attachment type=\"dir\" path=\"subdir\">"));
        assert!(text.contains("</attachment>"));
    }
}
