use std::path::PathBuf;

use ignore::WalkBuilder;
use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

use anyhow::Result;

use crate::config::{ProjectConfig, ProviderConfig};
use crate::dirs::{GlobalGantryDir, ProjectRootDir};
use crate::events::AppEvent;
use crate::fs::FsSessionRegistry;
use crate::message::Message;
use crate::metrics::{CharCounts, ContextWindow, Usage};
use crate::provider::registry::ProviderClientRegistry;
use crate::provider::{ModelId, ModelSelection};
use crate::session::registry::SessionRegistry;
use crate::session::{NodeId, Session, SessionId, SessionTree};
use crate::skills::{Skill, load_skills};
use crate::system_prompt::SystemPrompt;
use crate::tools::{BashTool, EditTool, GrepTool, ReadTool, TreeTool, WriteTool};
use rig::completion::Usage as RigUsage;
use rig::tool::ToolDyn;
use tokio::sync::broadcast;

type FsSession = Session<crate::fs::session_registry::FsSessionHistory>;

/// Central coordinator for an active gantry session.
///
/// Owns the active conversation session, the current model selection, the project path, and the
/// provider registry. All chat and session operations go through this type.
pub struct App {
    pub project_path: PathBuf,
    pub project_name: String,
    pub(crate) cwd: PathBuf,
    sessions_dir: PathBuf,
    /// `None` when a new session is pending — created lazily on the first `append_message` call.
    session: Option<FsSession>,
    pub selection: Option<ModelSelection>,
    pub(crate) registry: ProviderClientRegistry,
    pub(crate) system_prompt: SystemPrompt,
    /// Token usage from the most recently completed stream.
    pub(crate) last_usage: Option<RigUsage>,
    /// Character counts per component, captured just before the most recent request.
    pub(crate) last_char_counts: Option<CharCounts>,
    /// Accumulated token consumption across all nodes in the active session.
    total_usage: Usage,
    event_tx: broadcast::Sender<AppEvent>,
}

impl App {
    /// Creates an `App` for the given project root, resuming the most recent session or leaving
    /// the session pending (created on first message) if none exist.
    ///
    /// Sessions are stored under `global_config_dir/sessions/<project_name>/`.
    pub fn new(
        global_config_dir: GlobalGantryDir,
        project_root_dir: ProjectRootDir,
        cwd: PathBuf,
        registry: ProviderClientRegistry,
    ) -> Result<Self> {
        let default_model = registry.providers.catalog.default_model.clone();
        let project_path = project_root_dir.path().to_path_buf();
        let project_config = ProjectConfig::load(&project_root_dir.config_file())?;
        let sessions_dir = global_config_dir.sessions_dir(&project_config.name);
        let session_registry = FsSessionRegistry::new(&sessions_dir)?;
        let sessions = session_registry.list()?;

        let session = sessions
            .last()
            .map(|last| session_registry.load_session(&last.id))
            .transpose()?;
        let total_consumption = session
            .as_ref()
            .map(|s| s.total_consumption())
            .unwrap_or_default();
        let system_prompt = SystemPrompt::new(&cwd);
        let (event_tx, _) = broadcast::channel(64);

        Ok(Self {
            project_path,
            project_name: project_config.name,
            cwd,
            sessions_dir,
            session,
            selection: default_model,
            registry,
            system_prompt,
            last_usage: None,
            last_char_counts: None,
            total_usage: total_consumption,
            event_tx,
        })
    }

    /// Returns a new receiver for out-of-band app events emitted by tools.
    pub fn subscribe_events(&self) -> broadcast::Receiver<AppEvent> {
        self.event_tx.subscribe()
    }

    /// Returns a clone of the event sender for injecting mock events (e.g. in tests or debug mode).
    pub fn event_sender(&self) -> broadcast::Sender<AppEvent> {
        self.event_tx.clone()
    }

    /// Lists all sessions for this project, sorted by creation time (oldest first).
    pub fn list_sessions(&self) -> Result<Vec<crate::session::registry::SessionInfo>> {
        FsSessionRegistry::new(&self.sessions_dir)?.list()
    }

    /// Switches the active session to the one identified by `session_id`.
    pub fn resume_session(&mut self, session_id: &SessionId) -> Result<()> {
        let session_registry = FsSessionRegistry::new(&self.sessions_dir)?;
        let session = session_registry.load_session(session_id)?;
        self.total_usage = session.total_consumption();
        self.session = Some(session);
        self.last_usage = None;
        self.last_char_counts = None;
        Ok(())
    }

    /// Returns the ID of the active session, or `None` if no session has been started yet.
    pub fn session_id(&self) -> Option<&SessionId> {
        self.session.as_ref().map(|s| &s.session_id)
    }

    /// Marks the app as ready for a new session, created lazily on the next `append_message`.
    pub fn new_session(&mut self) -> Result<()> {
        self.session = None;
        self.total_usage = Usage::default();
        self.last_usage = None;
        self.last_char_counts = None;
        Ok(())
    }

    /// Appends a message, creating a new session first if none is active.
    pub fn append_message(&mut self, msg: Message) -> Result<()> {
        if self.session.is_none() {
            let session_registry = FsSessionRegistry::new(&self.sessions_dir)?;
            self.session = Some(session_registry.create_session(msg)?);
            return Ok(());
        }
        self.session.as_mut().unwrap().append_message(msg)
    }

    /// Appends a message with token usage to the active session, persisting it to disk.
    ///
    /// Panics if called before any message has been appended (i.e. while session is pending),
    /// since usage-bearing messages are always assistant replies that follow a user message.
    pub fn append_message_with_usage(&mut self, msg: Message, usage: Option<Usage>) -> Result<()> {
        self.session
            .as_mut()
            .expect("append_message_with_usage called before session was created")
            .append_message_with_usage(msg, usage)
    }

    /// Returns the accumulated token consumption across all nodes in the active session.
    pub fn total_usage(&self) -> &Usage {
        &self.total_usage
    }

    /// Returns the ordered messages on the active branch, or an empty vec if no session is active.
    pub fn history(&self) -> Vec<Message> {
        self.session
            .as_ref()
            .map(|s| s.history())
            .unwrap_or_default()
    }

    /// Builds and returns the session tree, or `None` if no session is active.
    pub fn get_tree(&self) -> Option<SessionTree> {
        self.session.as_ref().map(|s| s.as_tree())
    }

    /// Switches the active leaf to the node identified by `node_id_str`.
    ///
    /// Panics if no session is active.
    pub fn branch(&mut self, node_id_str: &str) -> Result<()> {
        let node_id: NodeId = node_id_str.parse()?;
        self.session
            .as_mut()
            .expect("branch called before session was created")
            .branch(&node_id)
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
            .find(|p| p.alias() == &selection.provider_alias)
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
        let provider_aliases: Vec<_> = self
            .registry
            .providers()
            .iter()
            .map(|p| p.alias().clone())
            .collect();

        let mut selections = Vec::new();
        let mut errors = Vec::new();
        // TODO: We need to have a way to filter out non completion models but I am not sure if
        // the `type` field contains stable keys. it's just a string, not an enum.
        for provider_alias in provider_aliases {
            match self.registry.client(&provider_alias) {
                Err(e) => errors.push(format!("{}: {}", provider_alias.as_str(), e)),
                Ok(client) => match client.list_models().await {
                    Err(e) => errors.push(format!("{}: {}", provider_alias.as_str(), e)),
                    Ok(list) => {
                        for model in list.data {
                            let mut selection = ModelSelection {
                                provider_alias: provider_alias.clone(),
                                model_id: ModelId::new(model.id),
                                context_length: model.context_length,
                            };
                            if selection.context_length.is_none() {
                                selection.context_length = self.resolve_context_length(&selection);
                            }
                            selections.push(selection);
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
    pub fn set_active_model(&mut self, model_alias: ModelId) -> Result<()> {
        let provider_alias = self
            .selection
            .as_ref()
            .map(|s| s.provider_alias.clone())
            .ok_or_else(|| anyhow::anyhow!("no active model selection"))?;
        self.set_selection(ModelSelection {
            provider_alias,
            model_id: model_alias,
            context_length: None,
        });
        Ok(())
    }

    /// Rebuilds the cached system prompt from disk.
    pub fn refresh_system_prompt(&mut self) {
        self.system_prompt.refresh(&self.cwd);
    }

    /// Persists a completed assistant turn and updates token usage and consumption totals.
    pub fn commit_response(&mut self, text: String, usage: Option<RigUsage>) {
        if let Some(u) = usage {
            let consumption = Usage::from(&u);
            if !text.is_empty() {
                let _ = self
                    .append_message_with_usage(Message::assistant(text), Some(consumption.clone()));
            }
            self.total_usage += consumption;
            self.last_usage = Some(u);
        } else if !text.is_empty() {
            let _ = self.append_message(Message::assistant(text));
        }
    }

    /// Returns the tools available for the current request.
    pub fn tools(&self) -> Vec<Box<dyn ToolDyn>> {
        let cwd = self.project_path.clone();
        vec![
            Box::new(ReadTool { cwd: cwd.clone() }),
            Box::new(WriteTool { cwd: cwd.clone() }),
            Box::new(EditTool {
                cwd: cwd.clone(),
                event_tx: self.event_tx.clone(),
            }),
            // Box::new(GrepTool { cwd: cwd.clone() }),
            // Box::new(TreeTool { cwd: cwd.clone() }),
            Box::new(BashTool { cwd }),
        ]
    }

    /// Returns all file and directory paths under the project root matching `query`.
    ///
    /// Walks the project root respecting `.gitignore`. Results are sorted by descending
    /// nucleo score; all paths are returned when `query` is empty. Each result includes
    /// the matched character indices into the normalized relative path string.
    pub fn search_paths(&self, query: &str) -> Vec<PathSearchResult> {
        let paths: Vec<PathBuf> = WalkBuilder::new(&self.project_path)
            .hidden(true)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.path() != self.project_path)
            .map(|e| e.into_path())
            .collect();

        if query.is_empty() {
            return paths
                .into_iter()
                .map(|path| PathSearchResult {
                    path,
                    indices: vec![],
                })
                .collect();
        }

        // TODO: I am not sure if this normalization is actually a good idea but it works for now.
        let normalized_query = query.replace(['/', '-', '_'], " ");
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::new(
            &normalized_query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let mut scored: Vec<(u32, usize, PathBuf, Vec<u32>)> = paths
            .into_iter()
            .filter_map(|p| {
                let rel = p.strip_prefix(&self.project_path).unwrap_or(&p);
                // Normalize path separators and word-boundary chars to spaces so that a query
                // like "tools edit" or "tools-edit" scores against "gantry-tools/src/edit.rs"
                // as if it were "gantry tools src edit.rs", enabling cross-component matching.
                let normalized = rel.to_string_lossy().replace(['/', '-', '_'], " ");
                let depth = rel.components().count().saturating_sub(1);
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new(&normalized, &mut Vec::new()),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                Some((score, depth, p, indices))
            })
            .collect();

        // Sort by descending score, then ascending depth as tiebreaker.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored
            .into_iter()
            .map(|(_, _, path, indices)| PathSearchResult { path, indices })
            .collect()
    }

    /// Returns skills whose names match `query`, sorted by descending nucleo score.
    ///
    /// All skills are returned when `query` is empty. Each result includes the matched
    /// character indices into the skill name string.
    pub fn search_skills(&self, query: &str) -> Vec<SkillSearchResult> {
        let mut skills = load_skills(&self.cwd).unwrap_or_default();

        if query.is_empty() {
            skills.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
            return skills
                .into_iter()
                .map(|skill| SkillSearchResult {
                    skill,
                    indices: vec![],
                })
                .collect();
        }

        let normalized_query = query.replace(['-', '_'], " ");
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::new(
            &normalized_query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let mut scored: Vec<(u32, Skill, Vec<u32>)> = skills
            .into_iter()
            .filter_map(|s| {
                let normalized = s.metadata.name.replace(['-', '_'], " ");
                let mut indices = Vec::new();
                let score = pattern.indices(
                    nucleo_matcher::Utf32Str::new(&normalized, &mut Vec::new()),
                    &mut matcher,
                    &mut indices,
                )?;
                indices.sort_unstable();
                Some((score, s, indices))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored
            .into_iter()
            .map(|(_, skill, indices)| SkillSearchResult { skill, indices })
            .collect()
    }
}

/// A single result from [`App::search_paths`].
pub struct PathSearchResult {
    pub path: PathBuf,
    /// Matched character indices into the normalized relative path string.
    pub indices: Vec<u32>,
}

/// A single result from [`App::search_skills`].
pub struct SkillSearchResult {
    pub skill: Skill,
    /// Matched character indices into the normalized skill name string.
    pub indices: Vec<u32>,
}
