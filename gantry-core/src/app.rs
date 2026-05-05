use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use rig::message::Message;
use tokio::sync::Mutex;

use crate::dirs::{ProjectConfigDir, ProjectRootDir};
use crate::fs::FsSessionRegistry;
use crate::project::resource_loader::discover_agents_md;
use crate::provider::agent_factory::{ChatStream, RigAgentFactory};
use crate::provider::{ModelId, ModelSelection, ProviderConfig, ProviderId};
use crate::session::registry::SessionRegistry;
use crate::session::{NodeId, Session, SessionId, SessionTree};
use crate::system_prompt::build_system_prompt;

type FsSession = Session<crate::fs::session_registry::FsSessionHistory>;

/// Central coordinator for an active gantry session.
///
/// Owns the active conversation session, the current model selection, the project path, and the
/// agent factory. All chat and session operations go through this type.
pub struct App {
    pub project_path: PathBuf,
    session: FsSession,
    selection: ModelSelection,
    agent_factory: RigAgentFactory,
}

impl App {
    /// Creates an `App` for the project at `project_path`, resuming the most recent session or
    /// creating a new one if none exist.
    pub fn new(
        project_path: &Path,
        selection: ModelSelection,
        agent_factory: RigAgentFactory,
    ) -> Result<Self> {
        let root = ProjectRootDir::new(project_path)?;
        let config_dir = ProjectConfigDir::new(&root)?;
        let registry = FsSessionRegistry::new(&config_dir)?;
        let sessions = registry.list()?;

        let session = if let Some(last) = sessions.last() {
            registry.load_session(&last.id)?
        } else {
            registry.create_session()?
        };

        Ok(Self {
            project_path: project_path.to_path_buf(),
            session,
            selection,
            agent_factory,
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
        let registry = FsSessionRegistry::new(&config_dir)?;
        self.session = registry.create_session()?;
        Ok(())
    }

    /// Appends a message to the active session, persisting it to disk.
    pub fn append_message(&mut self, msg: Message) -> Result<()> {
        self.session.append_message(msg)
    }

    /// Returns the ordered messages on the active branch.
    pub fn context_messages(&self) -> Vec<Message> {
        self.session.context_messages()
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

    /// Returns the active model selection.
    pub fn selection(&self) -> &ModelSelection {
        &self.selection
    }

    /// Replaces the active model selection.
    pub fn set_selection(&mut self, selection: ModelSelection) {
        self.selection = selection;
    }

    /// Returns all configured providers with their available models.
    pub fn list_providers(&self) -> Vec<ProviderConfig> {
        self.agent_factory.catalog().providers.clone()
    }

    /// Validates and sets the active provider, using its default model.
    pub fn set_active_provider(&mut self, provider_id: ProviderId) -> Result<()> {
        let model_id = self
            .agent_factory
            .catalog()
            .provider_default_model(&provider_id)?
            .clone();
        self.set_selection(ModelSelection {
            provider_id,
            model_id,
        });
        Ok(())
    }

    /// Validates and sets the active model, keeping the current provider.
    pub fn set_active_model(&mut self, model_id: ModelId) -> Result<()> {
        let provider_id = self.selection.provider_id.clone();
        let selection = ModelSelection {
            provider_id,
            model_id,
        };
        self.agent_factory.catalog().selection(&selection)?;
        self.set_selection(selection);
        Ok(())
    }

    /// Persists `content` as a user message, then returns rig's streaming result for the agent
    /// response. The caller is responsible for persisting the assistant message after the stream
    /// completes.
    pub async fn stream_message(
        app: Arc<Mutex<App>>,
        content: String,
    ) -> Result<ChatStream> {
        let mut app = app.lock().await;
        app.append_message(Message::user(content))?;
        let history = app.context_messages();
        let system_prompt = build_system_prompt(&discover_agents_md(&app.project_path));
        let agent = app.agent_factory.agent(&app.selection, Some(&system_prompt)).await?;
        let Some(prompt) = history.last().cloned() else {
            anyhow::bail!("no messages to stream");
        };
        let history = history[..history.len() - 1].to_vec();
        Ok(agent.stream_chat(prompt, history).await)
    }
}
