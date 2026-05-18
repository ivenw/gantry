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

use crate::message::{Attachment, Message};

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
/// read from disk and stored as [`Attachment`] values on the message. Attachment paths
/// are stored absolute; callers are responsible for making them relative for display.
/// The attachment content is appended as XML when the message is converted to a rig
/// message for the agent.
pub async fn build_user_message(tokens: Vec<InputToken>) -> Result<Message> {
    let mut body = String::new();
    let mut attachments = Vec::new();

    for token in tokens {
        match token {
            InputToken::Text(text) => body.push_str(&text),
            InputToken::Path(path) => {
                // TODO: The path here is absolute. In a live session it will be shown as relative
                // in the user message constructed by the TUI but on session reload it will right
                // now be shown as absolute. I have to think about how I want to handle this
                // inconsistency.
                body.push_str(format!("+{}", path.display()).as_str());
                let attachment = expand_path(&path)
                    .with_context(|| format!("failed to read attachment: {}", path.display()))?;
                attachments.push(attachment);
            }
            InputToken::Skill { name, path } => {
                body.push_str(format!("/{}", name).as_str());
                let attachment = expand_skill(name, &path)
                    .with_context(|| format!("failed to read skill: {}", path.display()))?;
                attachments.push(attachment);
            }
        }
    }

    Ok(Message::user_with_attachments(body, attachments))
}

/// Reads a path (file or directory) and returns an [`Attachment`] with its content.
///
/// The absolute path is stored on the attachment; callers are responsible for making
/// it relative for display purposes.
fn expand_path(path: &Path) -> Result<Attachment> {
    if path.is_dir() {
        let content =
            tree(path, None).with_context(|| format!("tree failed for {}", path.display()))?;
        Ok(Attachment::Dir {
            path: path.to_path_buf(),
            content,
        })
    } else {
        let content = read_file(path, None, None)
            .with_context(|| format!("read failed for {}", path.display()))?;
        Ok(Attachment::File {
            path: path.to_path_buf(),
            content,
        })
    }
}

/// Reads a skill's `SKILL.md` and returns an [`Attachment`] with its content.
fn expand_skill(name: String, path: &Path) -> Result<Attachment> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("read failed for {}", path.display()))?;
    Ok(Attachment::Skill { name, content })
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
        let msg = build_user_message(tokens).await.unwrap();
        assert_eq!(msg.text(), "hello world");
        assert!(msg.attachments().is_empty());
    }

    #[tokio::test]
    async fn file_token_produces_file_attachment() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("foo.txt");
        std::fs::File::create(&file_path)
            .unwrap()
            .write_all(b"line one\nline two\n")
            .unwrap();
        let tokens = vec![
            InputToken::Text("check this: ".to_string()),
            InputToken::Path(file_path.clone()),
        ];
        let msg = build_user_message(tokens).await.unwrap();
        assert_eq!(msg.text(), "check this: ");
        let attachments = msg.attachments();
        assert_eq!(attachments.len(), 1);
        // Path stored as absolute.
        assert!(matches!(&attachments[0], Attachment::File { path, content }
            if *path == file_path && content.contains("line one")));
        // Verify XML is produced when converting to rig format (absolute path in XML).
        let abs_path = file_path.display().to_string();
        let rig_msg: rig::message::Message = msg.into();
        if let rig::message::Message::User { content } = rig_msg {
            let text = content
                .into_iter()
                .find_map(|c| {
                    if let rig::message::UserContent::Text(t) = c {
                        Some(t.text)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(text.contains(&format!("<attachment type=\"file\" path=\"{}\">", abs_path)));
            assert!(text.contains("</attachment>"));
        } else {
            panic!("expected user message");
        }
    }

    #[tokio::test]
    async fn dir_token_produces_dir_attachment() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("subdir");
        std::fs::create_dir(&sub).unwrap();
        let tokens = vec![InputToken::Path(sub.clone())];
        let msg = build_user_message(tokens).await.unwrap();
        assert!(msg.text().is_empty());
        let attachments = msg.attachments();
        assert_eq!(attachments.len(), 1);
        // Path stored as absolute.
        assert!(matches!(&attachments[0], Attachment::Dir { path, .. }
            if *path == sub));
    }
}
