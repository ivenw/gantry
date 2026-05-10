use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::message::Message;
use anyhow::Result;
use futures::Stream;
use rig::agent::{MultiTurnStreamItem, StreamingError};
use rig::message::{AssistantContent, UserContent};
use rig::streaming::StreamedAssistantContent;
use tokio::sync::Mutex;
use tokio::sync::oneshot;

use crate::config::{ProjectConfig, ProviderConfig};
use crate::dirs::{GlobalConfigDir, ProjectRootDir};
use crate::fs::FsSessionRegistry;
use crate::metrics::{CharCounts, ContextWindow, Usage};
use crate::provider::agent::ChatStream;
use crate::provider::registry::ProviderClientRegistry;
use crate::provider::{HookEvent, ModelAlias, ModelSelection, PromptHook};
use crate::resource_loader::{load_context_files, load_skills};
use crate::session::registry::SessionRegistry;
use crate::session::{NodeId, Session, SessionId, SessionTree};
use crate::system_prompt::{BASE_PROMPT, build_system_prompt};
use crate::tools::{BashTool, EditTool, GrepTool, ReadTool, TreeTool, WriteTool};
use rig::completion::Usage as RigUsage;
use rig::tool::ToolDyn;

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
    /// Cached system prompt (preamble), rebuilt by [`App::refresh_system_prompt`].
    system_prompt: String,
    /// Char counts per agent file, captured when the system prompt was last built.
    agent_file_char_counts: Vec<(PathBuf, usize)>,
    /// Total chars contributed by the skills catalog, captured when the system prompt was last built.
    skills_catalog_char_count: usize,
    /// Token usage from the most recently completed stream.
    last_usage: Option<RigUsage>,
    /// Character counts per component, captured just before the most recent request.
    last_char_counts: Option<CharCounts>,
    /// Accumulated token consumption across all nodes in the active session.
    total_consumption: Usage,
}

impl App {
    /// Creates an `App` for the given project root, resuming the most recent session or creating a
    /// new one if none exist. The initial model selection is loaded from `~/.gantry/config.toml`.
    ///
    /// Sessions are stored under `global_config_dir/sessions/<project_name>/`.
    pub fn new(
        global_config_dir: GlobalConfigDir,
        project_root_dir: ProjectRootDir,
        registry: ProviderClientRegistry,
    ) -> Result<Self> {
        let default_model = registry.providers.catalog.default_model.clone();
        let project_path = project_root_dir.path().to_path_buf();
        let project_config = ProjectConfig::load(&project_root_dir.config_file())?;
        let sessions_dir = global_config_dir.sessions_dir(&project_config.name);
        let session_registry = FsSessionRegistry::new(&sessions_dir)?;
        let sessions = session_registry.list()?;

        let session = if let Some(last) = sessions.last() {
            session_registry.load_session(&last.id)?
        } else {
            session_registry.create_session()?
        };
        let total_consumption = session.total_consumption();
        let (system_prompt, agent_file_char_counts, skills_catalog_char_count) =
            Self::build_system_prompt_with_counts(&project_root_dir);

        Ok(Self {
            project_path,
            root: project_root_dir,
            sessions_dir,
            session,
            selection: default_model,
            registry,
            system_prompt,
            agent_file_char_counts,
            skills_catalog_char_count,
            last_usage: None,
            last_char_counts: None,
            total_consumption,
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
        self.total_consumption = self.session.total_consumption();
        self.last_usage = None;
        self.last_char_counts = None;
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
        self.total_consumption = Usage::default();
        self.last_usage = None;
        self.last_char_counts = None;
        Ok(())
    }

    /// Appends a message to the active session, persisting it to disk.
    pub fn append_message(&mut self, msg: Message) -> Result<()> {
        self.session.append_message(msg)
    }

    /// Appends a message with token usage to the active session, persisting it to disk.
    pub fn append_message_with_usage(&mut self, msg: Message, usage: Option<Usage>) -> Result<()> {
        self.session.append_message_with_usage(msg, usage)
    }

    /// Returns the accumulated token consumption across all nodes in the active session.
    pub fn total_consumption(&self) -> &Usage {
        &self.total_consumption
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

    /// Replaces the active model selection and persists it as the default in `config.toml`.
    ///
    /// If `selection.context_length` is `None`, attempts to resolve it from the provider config.
    /// For Ollama providers, the context window is read from `OllamaProviderConfig::context_window`.
    /// Persistence errors are silently ignored so a failed write never interrupts the session.
    pub fn set_selection(&mut self, mut selection: ModelSelection) {
        if selection.context_length.is_none() {
            selection.context_length = self.resolve_context_length(&selection);
        }
        let _ = self.registry.providers.save_default_model(&selection);
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
    /// been made yet or the model's max tokens are unknown. Combines last usage with the configured
    /// max tokens.
    pub fn context_window(&self) -> Option<ContextWindow> {
        let usage = self.last_usage.as_ref()?;
        let char_counts = self.last_char_counts.as_ref()?;
        if usage.total_tokens == 0 {
            return None;
        }
        let max_tokens = self.selection.as_ref().and_then(|s| s.context_length)?;
        Some(ContextWindow::new(usage, max_tokens, char_counts))
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

    /// Rebuilds the cached system prompt, agent file char counts, and skill catalog char count from
    /// disk.
    pub fn refresh_system_prompt(&mut self) {
        let (prompt, counts, skill_chars) = Self::build_system_prompt_with_counts(&self.root);
        self.system_prompt = prompt;
        self.agent_file_char_counts = counts;
        self.skills_catalog_char_count = skill_chars;
    }

    fn build_system_prompt_with_counts(
        project_root: &ProjectRootDir,
    ) -> (String, Vec<(PathBuf, usize)>, usize) {
        let agent_files = load_context_files(project_root).unwrap_or_default();
        let skills = load_skills(project_root).unwrap_or_default();
        let counts = agent_files
            .iter()
            .map(|f| (f.path.clone(), f.contents.len()))
            .collect();
        let skill_chars = skills
            .iter()
            .map(|s| s.metadata.name.len() + s.metadata.description.len())
            .sum();
        (build_system_prompt(&agent_files, &skills), counts, skill_chars)
    }

    /// Returns the tools available for the current request.
    pub fn tools(&self) -> Vec<Box<dyn ToolDyn>> {
        vec![
            Box::new(ReadTool),
            Box::new(WriteTool),
            Box::new(EditTool),
            Box::new(GrepTool),
            Box::new(TreeTool),
            Box::new(BashTool),
        ]
    }

    /// Appends `content` as a user message, captures char counts, and returns the prepared
    /// history and model selection needed to dispatch a request.
    ///
    /// Fails if no model selection is active.
    pub fn prepare_request(&mut self, content: String) -> Result<PreparedRequest> {
        self.append_message(Message::user(content))?;
        let history: Vec<rig::message::Message> =
            self.history().into_iter().map(Into::into).collect();
        let char_counts = CharCounts {
            base_prompt: BASE_PROMPT.len(),
            agent_files: self.agent_file_char_counts.clone(),
            skills_catalog: self.skills_catalog_char_count,
            messages: history.iter().fold(0, |acc, m| {
                acc + match m {
                    rig::message::Message::User { content } => content.iter().fold(0, |a, c| {
                        a + match c {
                            UserContent::Text(t) => t.text.len(),
                            _ => 0,
                        }
                    }),
                    rig::message::Message::Assistant { content, .. } => {
                        content.iter().fold(0, |a, c| {
                            a + match c {
                                AssistantContent::Text(t) => t.text.len(),
                                _ => 0,
                            }
                        })
                    }
                    rig::message::Message::System { content } => content.len(),
                }
            }),
        };
        self.last_char_counts = Some(char_counts);
        let selection = self
            .selection
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no active model selection"))?;
        Ok(PreparedRequest { history, selection })
    }
}

/// Prepared inputs for a single agent request, returned by [`App::prepare_request`].
pub struct PreparedRequest {
    pub history: Vec<rig::message::Message>,
    pub selection: ModelSelection,
}

/// Returned by [`stream_message`]. Holds the live stream and the deferred commit.
pub struct StreamingResponse {
    /// Yields streamed assistant content as it arrives.
    pub stream: ChatStream,
    commit_future: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
}

impl StreamingResponse {
    /// Drops the stream and persists the buffered assistant reply to the session.
    ///
    /// Must be called after the stream is consumed or abandoned. Safe to call after an
    /// interrupt — whatever was buffered up to that point will be persisted.
    pub async fn commit(self) {
        drop(self.stream);
        self.commit_future.await;
    }
}

/// Persists `content` as a user message, then streams the agent response.
///
/// Returns a [`StreamingResponse`] and a receiver for tool call lifecycle events. The caller
/// must call [`StreamingResponse::commit`] after the stream is done or abandoned to persist
/// the assistant reply and update token usage.
pub async fn stream_message(
    app: Arc<Mutex<App>>,
    content: String,
) -> Result<(
    StreamingResponse,
    tokio::sync::mpsc::UnboundedReceiver<HookEvent>,
)> {
    let (hook_tx, hook_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut guard = app.lock().await;
    let PreparedRequest { history, selection } = guard.prepare_request(content)?;
    let system_prompt = guard.system_prompt.clone();
    let tools = guard.tools();
    let hook = PromptHook::new(hook_tx);
    let agent = guard
        .registry
        .agent(&selection, Some(&system_prompt), hook, tools)?;
    let Some(prompt) = history.last().cloned() else {
        anyhow::bail!("no messages to stream");
    };
    let history = history[..history.len() - 1].to_vec();
    drop(guard);
    let (commit_tx, commit_rx) = oneshot::channel::<(String, Option<RigUsage>)>();
    let inner = agent.stream_chat(prompt, history).await;
    let stream = Box::pin(BufferingStream::new(inner, commit_tx));
    let commit_future = async move {
        let Ok((text, usage)) = commit_rx.await else {
            return;
        };
        let mut guard = app.lock().await;
        if let Some(u) = usage {
            let consumption = Usage::from(&u);
            if !text.is_empty() {
                let _ = guard
                    .append_message_with_usage(Message::assistant(text), Some(consumption.clone()));
            }
            guard.total_consumption += consumption;
            guard.last_usage = Some(u);
        } else if !text.is_empty() {
            let _ = guard.append_message(Message::assistant(text));
        }
    };
    Ok((
        StreamingResponse {
            stream,
            commit_future: Box::pin(commit_future),
        },
        hook_rx,
    ))
}

/// Wraps a [`ChatStream`], accumulating streamed text and usage. On drop (whether the stream
/// was fully consumed or interrupted), sends the buffer through a oneshot channel so the
/// caller's commit future can persist the assistant reply.
struct BufferingStream {
    inner: ChatStream,
    commit_tx: Option<oneshot::Sender<(String, Option<RigUsage>)>>,
    buffer: String,
    usage: Option<RigUsage>,
}

impl BufferingStream {
    fn new(inner: ChatStream, commit_tx: oneshot::Sender<(String, Option<RigUsage>)>) -> Self {
        Self {
            inner,
            commit_tx: Some(commit_tx),
            buffer: String::new(),
            usage: None,
        }
    }

    fn send_commit(&mut self) {
        if let Some(tx) = self.commit_tx.take() {
            let text = std::mem::take(&mut self.buffer);
            let usage = self.usage.take();
            let _ = tx.send((text, usage));
        }
    }
}

impl Drop for BufferingStream {
    fn drop(&mut self) {
        self.send_commit();
    }
}

impl Stream for BufferingStream {
    type Item = Result<MultiTurnStreamItem<()>, StreamingError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
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
                self.send_commit();
                Poll::Ready(None)
            }
        }
    }
}
