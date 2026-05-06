use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::message::Message;
use anyhow::Result;
use tokio::sync::Mutex;

use crate::config::ProviderConfig;
use crate::dirs::{ProjectConfigDir, ProjectRootDir};
use crate::fs::FsSessionRegistry;
use crate::project::resource_loader::discover_agents_md;
use crate::provider::agent::ChatStream;
use crate::provider::registry::ProviderClientRegistry;
use crate::provider::{ModelAlias, ModelSelection};
use crate::session::registry::SessionRegistry;
use crate::session::{NodeId, Session, SessionId, SessionTree};
use crate::system_prompt::build_system_prompt;

type FsSession = Session<crate::fs::session_registry::FsSessionHistory>;

/// Central coordinator for an active gantry session.
///
/// Owns the active conversation session, the current model selection, the project path, and the
/// provider registry. All chat and session operations go through this type.
pub struct App {
    pub project_path: PathBuf,
    session: FsSession,
    pub selection: Option<ModelSelection>,
    registry: ProviderClientRegistry,
}

impl App {
    /// Creates an `App` for the project at `project_path`, resuming the most recent session or
    /// creating a new one if none exist. `selection` is the initial model selection, if any.
    pub fn new(
        project_path: &Path,
        selection: Option<ModelSelection>,
        registry: ProviderClientRegistry,
    ) -> Result<Self> {
        let root = ProjectRootDir::new(project_path)?;
        let config_dir = ProjectConfigDir::new(&root)?;
        let session_registry = FsSessionRegistry::new(&config_dir)?;
        let sessions = session_registry.list()?;

        let session = if let Some(last) = sessions.last() {
            session_registry.load_session(&last.id)?
        } else {
            session_registry.create_session()?
        };

        Ok(Self {
            project_path: project_path.to_path_buf(),
            session,
            selection,
            registry,
        })
    }

    /// Returns the ID of the active session.
    pub fn session_id(&self) -> &SessionId {
        &self.session.session_id
    }

    /// Creates a new session and makes it active.
    pub fn new_session(&mut self) -> Result<()> {
        let root = ProjectRootDir::new(&self.project_path)?;
        let config_dir = ProjectConfigDir::new(&root)?;
        let session_registry = FsSessionRegistry::new(&config_dir)?;
        self.session = session_registry.create_session()?;
        Ok(())
    }

    /// Appends a message to the active session, persisting it to disk.
    pub fn append_message(&mut self, msg: Message) -> Result<()> {
        self.session.append_message(msg)
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
    pub fn set_selection(&mut self, selection: ModelSelection) {
        self.selection = Some(selection);
    }

    /// Returns all configured providers.
    pub fn list_providers(&self) -> &[ProviderConfig] {
        self.registry.providers()
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
        });
        Ok(())
    }

    /// Persists `content` as a user message, then returns rig's streaming result for the agent
    /// response. The caller is responsible for persisting the assistant message after the stream
    /// completes.
    pub async fn stream_message(app: Arc<Mutex<App>>, content: String) -> Result<ChatStream> {
        let mut app = app.lock().await;
        app.append_message(Message::user(content))?;
        let history: Vec<rig::message::Message> =
            app.history().into_iter().map(Into::into).collect();
        let system_prompt = build_system_prompt(&discover_agents_md(&app.project_path));
        let selection = app
            .selection
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no active model selection"))?;
        let agent = app.registry.agent(&selection, Some(&system_prompt))?;
        let Some(prompt) = history.last().cloned() else {
            anyhow::bail!("no messages to stream");
        };
        let history = history[..history.len() - 1].to_vec();
        Ok(agent.stream_chat(prompt, history).await)
    }
}
