use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::message::Message;
use anyhow::Result;
use futures::Stream;
use rig::agent::{MultiTurnStreamItem, StreamingError};
use rig::streaming::StreamedAssistantContent;
use tokio::sync::Mutex;

use crate::config::{ProjectConfig, ProviderConfig};
use crate::dirs::{GlobalConfigDir, ProjectRootDir};
use crate::fs::FsSessionRegistry;
use crate::metrics::{CharCounts, ContextWindow, RequestUsage};
use crate::provider::agent::ChatStream;
use crate::provider::registry::ProviderClientRegistry;
use crate::provider::{ModelAlias, ModelSelection, ToolCallEvent};
use crate::resource_loader::discover_agents_md;
use crate::session::registry::SessionRegistry;
use crate::session::{NodeId, Session, SessionId, SessionTree};
use crate::system_prompt::{BASE_PROMPT, build_system_prompt};
use rig::completion::Usage;

type FsSession = Session<crate::fs::session_registry::FsSessionHistory>;

/// Central coordinator for an active gantry session.
///
/// Owns the active conversation session, the current model selection, the project path, and the
/// provider registry. All chat and session operations go through this type.
pub struct App {
    pub project_path: PathBuf,
    root: ProjectRootDir,
    sessions_dir: PathBuf,
    session: FsSession,
    pub selection: Option<ModelSelection>,
    registry: ProviderClientRegistry,
    /// Token usage from the most recently completed stream.
    last_usage: Option<Usage>,
    /// Character counts per component, captured just before the most recent request.
    last_char_counts: Option<CharCounts>,
}

impl App {
    /// Creates an `App` for the given project root, resuming the most recent session or creating a
    /// new one if none exist. `selection` is the initial model selection, if any.
    ///
    /// Sessions are stored under `global_config_dir/sessions/<project_name>/`.
    pub fn new(
        global_config_dir: GlobalConfigDir,
        project_rood_dir: ProjectRootDir,
        selection: Option<ModelSelection>,
        registry: ProviderClientRegistry,
    ) -> Result<Self> {
        let project_path = project_rood_dir.path().to_path_buf();
        let project_config = ProjectConfig::load(&project_rood_dir.config_file())?;
        let sessions_dir = global_config_dir.sessions_dir(&project_config.name);
        let session_registry = FsSessionRegistry::new(&sessions_dir)?;
        let sessions = session_registry.list()?;

        let session = if let Some(last) = sessions.last() {
            session_registry.load_session(&last.id)?
        } else {
            session_registry.create_session()?
        };

        Ok(Self {
            project_path,
            root: project_rood_dir,
            sessions_dir,
            session,
            selection,
            registry,
            last_usage: None,
            last_char_counts: None,
        })
    }

    /// Lists all sessions for this project, sorted by creation time (oldest first).
    pub fn list_sessions(&self) -> Result<Vec<crate::session::registry::SessionInfo>> {
        FsSessionRegistry::new(&self.sessions_dir)?.list()
    }

    /// Switches the active session to the one identified by `session_id`.
    pub fn resume_session(&mut self, session_id: &SessionId) -> Result<()> {
        let session_registry = FsSessionRegistry::new(&self.sessions_dir)?;
        self.session = session_registry.load_session(session_id)?;
        Ok(())
    }

    /// Returns the ID of the active session.
    pub fn session_id(&self) -> &SessionId {
        &self.session.session_id
    }

    /// Creates a new session and makes it active.
    pub fn new_session(&mut self) -> Result<()> {
        let session_registry = FsSessionRegistry::new(&self.sessions_dir)?;
        self.session = session_registry.create_session()?;
        Ok(())
    }

    /// Appends a message to the active session, persisting it to disk.
    pub fn append_message(&mut self, msg: Message) -> Result<()> {
        self.session.append_message(msg)
    }

    /// Appends a message with token usage to the active session, persisting it to disk.
    pub fn append_message_with_usage(&mut self, msg: Message, usage: Option<RequestUsage>) -> Result<()> {
        self.session.append_message_with_usage(msg, usage)
    }

    /// Returns all request usage records on the active branch, in chronological order.
    pub fn usage_history(&self) -> Vec<RequestUsage> {
        self.session
            .all_nodes()
            .filter_map(|n| n.usage.clone())
            .collect()
    }

    /// Returns the ordered messages on the active branch.
    pub fn history(&self) -> Vec<Message> {
        self.session.history()
    }

    /// Builds and returns the session tree, or `None` if the session has no nodes.
    pub fn get_tree(&self) -> Option<SessionTree> {
        self.session.as_tree()
    }

    /// Switches the active leaf to the node identified by `node_id_str`.
    pub fn branch(&mut self, node_id_str: &str) -> Result<()> {
        let node_id: NodeId = node_id_str.parse()?;
        self.session.branch(&node_id)
    }

    /// Returns the active model selection, if one has been set.
    pub fn selection(&self) -> Option<&ModelSelection> {
        self.selection.as_ref()
    }

    /// Replaces the active model selection.
    ///
    /// If `selection.context_length` is `None`, attempts to resolve it from the provider config.
    /// For Ollama providers, the context window is read from `OllamaProviderConfig::context_window`.
    pub fn set_selection(&mut self, mut selection: ModelSelection) {
        if selection.context_length.is_none() {
            selection.context_length = self.resolve_context_length(&selection);
        }
        self.selection = Some(selection);
    }

    /// Resolves the context window size for a selection from the provider config.
    fn resolve_context_length(&self, selection: &ModelSelection) -> Option<u32> {
        self.registry
            .providers()
            .iter()
            .find(|p| p.alias() == &selection.provider)
            .and_then(|p| match p {
                ProviderConfig::Ollama(cfg) => cfg.context_length,
                _ => None,
            })
    }

    /// Returns a context window snapshot for the most recent request, or `None` if no request has
    /// been made yet. Combines last usage with the configured context length, if available.
    pub fn context_window(&self) -> Option<ContextWindow> {
        let usage = self.last_usage.as_ref()?;
        let char_counts = self.last_char_counts.as_ref()?;
        if usage.total_tokens == 0 {
            return None;
        }
        let context_length = self.selection.as_ref().and_then(|s| s.context_length);
        Some(ContextWindow::new(usage, context_length, char_counts))
    }

    /// Returns all configured providers.
    pub fn list_providers(&self) -> &[ProviderConfig] {
        self.registry.providers()
    }

    /// Lists all available models across every configured provider.
    ///
    /// Queries each provider in turn. Returns an error if any provider fails, including
    /// the alias and reason for each failure. On success the selections are ordered by
    /// provider, then by model within that provider.
    pub async fn list_models(&mut self) -> Result<Vec<ModelSelection>> {
        let aliases: Vec<_> = self
            .registry
            .providers()
            .iter()
            .map(|p| p.alias().clone())
            .collect();

        let mut selections = Vec::new();
        let mut errors = Vec::new();
        for alias in aliases {
            match self.registry.client(&alias) {
                Err(e) => errors.push(format!("{}: {}", alias.as_str(), e)),
                Ok(client) => match client.list_models().await {
                    Err(e) => errors.push(format!("{}: {}", alias.as_str(), e)),
                    Ok(list) => {
                        for model in list.data {
                            selections.push(ModelSelection {
                                provider: alias.clone(),
                                model: ModelAlias::new(model.id),
                                context_length: model.context_length,
                            });
                        }
                    }
                },
            }
        }

        if errors.is_empty() {
            Ok(selections)
        } else {
            Err(anyhow::anyhow!("{}", errors.join("; ")))
        }
    }

    /// Adds a new provider to `config.toml` and optionally saves its credential.
    ///
    /// Fails if a provider with the same alias already exists.
    pub fn add_provider(
        &mut self,
        config: ProviderConfig,
        credential: Option<crate::config::StoredCredential>,
    ) -> Result<()> {
        if let Some(cred) = credential {
            self.registry
                .credentials
                .save_credential(config.alias(), cred)?;
        }
        self.registry.providers.add_provider(config)
    }

    /// Removes a provider from `config.toml` and its credential from `credentials.toml`.
    ///
    /// The credential removal is best-effort: if no credential exists for the alias, it is
    /// silently skipped.
    pub fn remove_provider(&mut self, alias: &crate::provider::ProviderAlias) -> Result<()> {
        let _ = self.registry.credentials.remove_credential(alias);
        self.registry.providers.remove_provider(alias)
    }

    /// Validates and sets the active model, keeping the current provider.
    ///
    /// Returns an error if no selection is currently active.
    pub fn set_active_model(&mut self, model_alias: ModelAlias) -> Result<()> {
        let provider_alias = self
            .selection
            .as_ref()
            .map(|s| s.provider.clone())
            .ok_or_else(|| anyhow::anyhow!("no active model selection"))?;
        self.set_selection(ModelSelection {
            provider: provider_alias,
            model: model_alias,
            context_length: None,
        });
        Ok(())
    }

    /// Persists `content` as a user message, then streams the agent response.
    ///
    /// Returns the chat stream and a receiver for tool call lifecycle events. When the stream
    /// is exhausted the assembled assistant reply is automatically appended to the session
    /// history. The caller does not need to persist it separately.
    pub async fn stream_message(
        app: Arc<Mutex<App>>,
        content: String,
    ) -> Result<(
        ChatStream,
        tokio::sync::mpsc::UnboundedReceiver<ToolCallEvent>,
    )> {
        let (hook_tx, hook_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut guard = app.lock().await;
        guard.append_message(Message::user(content))?;
        let history: Vec<rig::message::Message> =
            guard.history().into_iter().map(Into::into).collect();
        let agent_files = discover_agents_md(&guard.project_path);
        let system_prompt = build_system_prompt(&agent_files);
        let char_counts = CharCounts {
            base_prompt: BASE_PROMPT.len(),
            agent_files: agent_files
                .iter()
                .map(|f| (f.path.clone(), f.contents.len()))
                .collect(),
            messages: history.iter().fold(0, |acc, m| {
                acc + match m {
                    rig::message::Message::User { content } => content.iter().fold(0, |a, c| {
                        a + match c {
                            rig::message::UserContent::Text(t) => t.text.len(),
                            _ => 0,
                        }
                    }),
                    rig::message::Message::Assistant { content, .. } => {
                        content.iter().fold(0, |a, c| {
                            a + match c {
                                rig::message::AssistantContent::Text(t) => t.text.len(),
                                _ => 0,
                            }
                        })
                    }
                    rig::message::Message::System { content } => content.len(),
                }
            }),
        };
        guard.last_char_counts = Some(char_counts);
        let selection = guard
            .selection
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no active model selection"))?;
        let agent = guard
            .registry
            .agent(&selection, Some(&system_prompt), hook_tx)?;
        let Some(prompt) = history.last().cloned() else {
            anyhow::bail!("no messages to stream");
        };
        let history = history[..history.len() - 1].to_vec();
        drop(guard);
        let inner = agent.stream_chat(prompt, history).await;
        Ok((Box::pin(AppendOnExhaust::new(inner, app)), hook_rx))
    }
}

/// Wraps a [`ChatStream`], accumulating streamed text and appending the complete assistant
/// message to the session once the inner stream is exhausted. Also captures token usage from
/// the [`MultiTurnStreamItem::FinalResponse`] and stores it on the [`App`].
struct AppendOnExhaust {
    inner: ChatStream,
    app: Arc<Mutex<App>>,
    buffer: String,
    usage: Option<Usage>,
    done: bool,
}

impl AppendOnExhaust {
    fn new(inner: ChatStream, app: Arc<Mutex<App>>) -> Self {
        Self {
            inner,
            app,
            buffer: String::new(),
            usage: None,
            done: false,
        }
    }
}

impl Stream for AppendOnExhaust {
    type Item = Result<MultiTurnStreamItem<()>, StreamingError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(item)) => {
                match &item {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Text(t),
                    )) => {
                        self.buffer.push_str(&t.text);
                    }
                    Ok(MultiTurnStreamItem::FinalResponse(f)) => {
                        self.usage = Some(f.usage());
                    }
                    _ => {}
                }
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                self.done = true;
                let text = std::mem::take(&mut self.buffer);
                let usage = self.usage.take();
                let app = self.app.clone();
                // Spawn a detached task so we can persist without blocking the stream poll.
                tokio::spawn(async move {
                    let mut guard = app.lock().await;
                    if let Some(u) = usage {
                        let request_usage = RequestUsage::from(&u);
                        if !text.is_empty() {
                            let _ = guard.append_message_with_usage(
                                Message::assistant(text),
                                Some(request_usage),
                            );
                        }
                        guard.last_usage = Some(u);
                    } else if !text.is_empty() {
                        let _ = guard.append_message(Message::assistant(text));
                    }
                });
                Poll::Ready(None)
            }
        }
    }
}
