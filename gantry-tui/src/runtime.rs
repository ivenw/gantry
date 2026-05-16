use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, KeyEventKind, MouseEventKind};
use futures::StreamExt;
use gantry_core::App;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;

use crate::chat::ChatMessage;
use crate::message::{Cmd, Msg};
use crate::model::{Model, SessionStats};
use crate::model::update;
use crate::view::{self, WidgetState};

pub struct Runtime {
    model: Model,
    rt: tokio::runtime::Runtime,
    msg_tx: Sender<Event>,
    msg_rx: Receiver<Event>,
    app: Arc<Mutex<App>>,
    view_state: WidgetState,
    stream_task: Option<JoinHandle<()>>,
    is_streaming: Arc<AtomicBool>,
    cancel_stream: Arc<AtomicBool>,
    _event_task: JoinHandle<()>,
}

impl Runtime {
    /// Creates a new runtime, loading existing messages from the app.
    pub fn new(app: Arc<Mutex<App>>, cwd: std::path::PathBuf) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let (msg_tx, msg_rx) = channel::<Event>(256);

        let (
            session_id,
            existing_messages,
            selection,
            project_path,
            project_name,
            session_stats,
            mut event_rx,
        ) = {
            let app = rt.block_on(app.lock());
            (
                app.session_id().cloned(),
                ChatMessage::messages_from(app.history()),
                app.selection().cloned(),
                app.project_path.clone(),
                app.project_name.clone(),
                SessionStats {
                    context_window: app.context_window(),
                    usage: app.total_consumption().clone(),
                },
                app.subscribe_events(),
            )
        };
        let model = Model::new(
            session_id,
            existing_messages,
            selection,
            project_path,
            project_name,
            session_stats,
            cwd,
        );

        let event_tx = msg_tx.clone();
        let event_task = rt.spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        if event_tx.send(Msg::AppEvent(event).into()).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });

        Ok(Self {
            model,
            rt,
            msg_tx,
            msg_rx,
            app,
            view_state: WidgetState::default(),
            stream_task: None,
            is_streaming: Arc::new(AtomicBool::new(false)),
            cancel_stream: Arc::new(AtomicBool::new(false)),
            _event_task: event_task,
        })
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        terminal.draw(|f| view::render(f, &mut self.model, &mut self.view_state))?;

        let tick_interval = Duration::from_millis(100);
        let mut last_tick = Instant::now();

        loop {
            if crossterm::event::poll(Duration::from_millis(10))? {
                match crossterm::event::read()? {
                    CrosstermEvent::Key(key)
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
                    {
                        let _ = self.msg_tx.try_send(Msg::KeyEvent(key).into());
                    }
                    CrosstermEvent::Mouse(mouse) => {
                        let delta: i32 = match mouse.kind {
                            MouseEventKind::ScrollUp => 1,
                            MouseEventKind::ScrollDown => -1,
                            _ => 0,
                        };
                        if delta != 0 {
                            let _ = self.msg_tx.try_send(Msg::ScrollChat(delta).into());
                        }
                    }
                    _ => {}
                }
            }

            let mut needs_redraw = false;
            while let Ok(msg) = self.msg_rx.try_recv() {
                needs_redraw = true;
                if self.dispatch(msg) {
                    return Ok(());
                }
            }

            if last_tick.elapsed() >= tick_interval {
                last_tick = Instant::now();
                self.view_state.throbber.tick(Instant::now());
                needs_redraw = true;
            }

            if needs_redraw {
                terminal.draw(|f| view::render(f, &mut self.model, &mut self.view_state))?;
            }
        }
    }

    fn dispatch(&mut self, event: Event) -> bool {
        let is_quit = matches!(event, Event::Cmd(Cmd::Quit));
        let mut next_cmd = self.process(event);
        while let Some(cmd) = next_cmd {
            next_cmd = match self.handle_cmd(cmd) {
                Some(msg) => self.process(Event::Msg(msg)),
                None => None,
            };
        }
        is_quit
    }

    /// Routes an event: `Msg` goes to `update()`, `Cmd` goes to `handle_cmd()`.
    fn process(&mut self, event: Event) -> Option<Cmd> {
        match event {
            Event::Msg(msg) => update(&mut self.model, &self.view_state, msg),
            Event::Cmd(cmd) => self
                .handle_cmd(cmd)
                .and_then(|msg| self.process(Event::Msg(msg))),
        }
    }

    /// Handles all side-effect messages, returning an optional follow-up message.
    fn handle_cmd(&mut self, msg: Cmd) -> Option<Msg> {
        match msg {
            Cmd::Quit => None,
            Cmd::NewSession => {
                match self
                    .rt
                    .block_on(async { self.app.lock().await.new_session() })
                {
                    Ok(()) => Some(Msg::SessionCreated),
                    Err(e) => Some(Msg::SetStatus(format!("failed to create session: {e}"))),
                }
            }
            Cmd::RunCommand(cmd) => {
                self.run_command(cmd);
                None
            }
            Cmd::InterruptStream => {
                self.interrupt_stream();
                Some(Msg::CancelStream)
            }
            Cmd::BranchTo(entry_id) => {
                self.spawn_branch(entry_id);
                None
            }
            Cmd::BranchToWithInput { branch_id, input } => {
                self.spawn_branch_with_input(branch_id, input);
                None
            }
            Cmd::SendMessage(tokens) => {
                self.spawn_send_message(tokens);
                Some(Msg::StartStream)
            }
            Cmd::OpenPathPicker(query) => {
                let paths = self
                    .rt
                    .block_on(async { self.app.lock().await.search_paths(&query) });
                Some(Msg::ActivatePathPicker(paths))
            }
            Cmd::OpenSkillPicker(query) => {
                let skills = self
                    .rt
                    .block_on(async { self.app.lock().await.search_skills(&query) });
                Some(Msg::ActivateSkillPicker(skills))
            }
            Cmd::RefineAttachmentPicker(query) => {
                if self.model.is_skill_attachment_picker_active() {
                    let skills = self
                        .rt
                        .block_on(async { self.app.lock().await.search_skills(&query) });
                    Some(Msg::SetSkillPickerResults(skills))
                } else {
                    let paths = self
                        .rt
                        .block_on(async { self.app.lock().await.search_paths(&query) });
                    Some(Msg::SetPathPickerResults(paths))
                }
            }
            Cmd::AddProvider(config, credential) => self.handle_add_provider(config, credential),
            Cmd::RemoveProvider(alias) => self.handle_remove_provider(alias),
            Cmd::SelectModel(selection) => self.handle_select_model(selection),
            Cmd::ResumeSession(session_id) => self.handle_resume_session(session_id),
        }
    }

    fn spawn_send_message(&mut self, input: Vec<gantry_core::InputToken>) {
        // Cancel any in-flight stream before starting a new one.
        self.cancel_stream.store(true, Ordering::SeqCst);
        if let Some(old) = self.stream_task.take() {
            old.abort();
        }

        let tx = self.msg_tx.clone();
        let app = self.app.clone();
        let app_ref = self.app.clone();
        let is_streaming = self.is_streaming.clone();
        let cancel = self.cancel_stream.clone();
        cancel.store(false, Ordering::SeqCst);

        let task = self.rt.spawn(async move {
            match gantry_core::stream_message(app, input.clone()).await {
                Err(e) => {
                    let _ = tx.send(Msg::StreamError(e.to_string()).into()).await;
                }
                Ok(mut response) => {
                    is_streaming.store(true, Ordering::SeqCst);
                    while let Some(item) = response.stream.next().await {
                        if cancel.load(Ordering::SeqCst)
                            || tx.send(Msg::StreamItem(item).into()).await.is_err()
                        {
                            break;
                        }
                    }
                    response.commit().await;
                    let stats = {
                        let app = app_ref.lock().await;
                        SessionStats {
                            context_window: app.context_window(),
                            usage: app.total_consumption().clone(),
                        }
                    };
                    is_streaming.store(false, Ordering::SeqCst);
                    let _ = tx.send(Msg::StreamDone(stats).into()).await;
                }
            }
        });
        self.stream_task = Some(task);
    }

    /// Starts a mock streaming response for testing the TUI rendering pipeline.
    fn spawn_mock_chat(&mut self) {
        self.cancel_stream.store(true, Ordering::SeqCst);
        if let Some(old) = self.stream_task.take() {
            old.abort();
        }

        let tx = self.msg_tx.clone();
        let is_streaming = self.is_streaming.clone();
        let cancel = self.cancel_stream.clone();
        cancel.store(false, Ordering::SeqCst);

        let event_tx = self
            .rt
            .block_on(async { self.app.lock().await.event_sender() });
        let task = self.rt.spawn(async move {
            let mut response = gantry_core::mock_stream_message(event_tx);
            is_streaming.store(true, Ordering::SeqCst);
            while let Some(item) = response.stream.next().await {
                if cancel.load(Ordering::SeqCst)
                    || tx.send(Msg::StreamItem(item).into()).await.is_err()
                {
                    break;
                }
            }
            response.commit().await;
            is_streaming.store(false, Ordering::SeqCst);
            let _ = tx
                .send(Msg::StreamDone(SessionStats::default()).into())
                .await;
        });
        self.stream_task = Some(task);
    }

    fn interrupt_stream(&mut self) {
        self.cancel_stream.store(true, Ordering::SeqCst);
        self.stream_task.take();
        self.is_streaming.store(false, Ordering::SeqCst);
    }

    fn spawn_branch(&mut self, entry_id: String) {
        let tx = self.msg_tx.clone();
        let app = self.app.clone();
        self.rt.spawn(async move {
            if let Err(e) = app.lock().await.branch(&entry_id) {
                let _ = tx
                    .send(Msg::SetStatus(format!("branch failed: {}", e)).into())
                    .await;
                return;
            }
            let messages = ChatMessage::messages_from(app.lock().await.history());
            let _ = tx.send(Msg::ReloadMessages(messages).into()).await;
        });
    }

    fn spawn_branch_with_input(&mut self, branch_id: String, input: String) {
        let tx = self.msg_tx.clone();
        let app = self.app.clone();
        self.rt.spawn(async move {
            if let Err(e) = app.lock().await.branch(&branch_id) {
                let _ = tx
                    .send(Msg::SetStatus(format!("branch failed: {}", e)).into())
                    .await;
                return;
            }
            let messages = ChatMessage::messages_from(app.lock().await.history());
            let _ = tx
                .send(Msg::ReloadMessagesWithInput(messages, input).into())
                .await;
        });
    }

    fn handle_add_provider(
        &mut self,
        config: gantry_core::ProviderConfig,
        credential: Option<gantry_core::StoredCredential>,
    ) -> Option<Msg> {
        match self
            .rt
            .block_on(async { self.app.lock().await.add_provider(config, credential) })
        {
            Ok(()) => {
                let providers = self
                    .rt
                    .block_on(async { self.app.lock().await.list_providers().to_vec() });
                Some(Msg::ProviderAdded(providers))
            }
            Err(e) => Some(Msg::ProviderAddFailed(e.to_string())),
        }
    }

    fn handle_remove_provider(&mut self, alias: gantry_core::ProviderAlias) -> Option<Msg> {
        match self
            .rt
            .block_on(async { self.app.lock().await.remove_provider(&alias) })
        {
            Ok(()) => {
                let providers = self
                    .rt
                    .block_on(async { self.app.lock().await.list_providers().to_vec() });
                Some(Msg::ProviderRemoved(providers))
            }
            Err(e) => Some(Msg::SetStatus(e.to_string())),
        }
    }

    fn handle_resume_session(&mut self, session_id: gantry_core::SessionId) -> Option<Msg> {
        let result = self
            .rt
            .block_on(async { self.app.lock().await.resume_session(&session_id) });
        match result {
            Err(e) => Some(Msg::SetStatus(format!("failed to resume session: {e}"))),
            Ok(()) => {
                let (messages, session_stats) = self.rt.block_on(async {
                    let app = self.app.lock().await;
                    let messages = ChatMessage::messages_from(app.history());
                    let session_stats = SessionStats {
                        context_window: app.context_window(),
                        usage: app.total_consumption().clone(),
                    };
                    (messages, session_stats)
                });
                Some(Msg::SessionLoaded {
                    session_id,
                    messages,
                    session_stats,
                })
            }
        }
    }

    fn handle_select_model(&mut self, selection: gantry_core::ModelSelection) -> Option<Msg> {
        self.rt.block_on(async {
            self.app.lock().await.set_selection(selection.clone());
        });
        Some(Msg::ModelSelected(selection))
    }

    /// Dispatches a `KnownCommand`, either immediately updating the model or spawning an async task.
    fn run_command(&mut self, cmd: crate::commands::KnownCommand) {
        use crate::commands::KnownCommand;
        match cmd {
            KnownCommand::Quit => {
                let _ = self.msg_tx.try_send(Cmd::Quit.into());
            }
            KnownCommand::New => {
                let _ = self.msg_tx.try_send(Cmd::NewSession.into());
            }
            KnownCommand::Model => {
                if let Some(models) = self.model.cached_models().map(|s| s.to_vec()) {
                    let _ = self.msg_tx.try_send(Msg::OpenModelPicker(models).into());
                    return;
                }
                let tx = self.msg_tx.clone();
                let app = self.app.clone();
                self.rt.spawn(async move {
                    match app.lock().await.list_models().await {
                        Ok(models) => {
                            let _ = tx.send(Msg::ModelsFetched(models).into()).await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Msg::SetStatus(format!("Failed to list models: {e}")).into())
                                .await;
                        }
                    }
                });
            }
            KnownCommand::Providers => {
                let tx = self.msg_tx.clone();
                let app = self.app.clone();
                self.rt.spawn(async move {
                    let providers = app.lock().await.list_providers().to_vec();
                    let _ = tx.send(Msg::OpenProviderConfig(providers).into()).await;
                });
            }
            KnownCommand::Sessions => {
                let tx = self.msg_tx.clone();
                let app = self.app.clone();
                self.rt.spawn(async move {
                    let app = app.lock().await;
                    match app.list_sessions() {
                        Ok(sessions) => {
                            if let Some(active_id) = app.session_id().cloned() {
                                let _ = tx
                                    .send(Msg::OpenSessionsPicker(sessions, active_id).into())
                                    .await;
                            }
                        }
                        Err(e) => {
                            let _ = tx
                                .send(
                                    Msg::SetStatus(format!("failed to list sessions: {e}")).into(),
                                )
                                .await;
                        }
                    }
                });
            }
            KnownCommand::Tree => {
                let tx = self.msg_tx.clone();
                let app = self.app.clone();
                self.rt.spawn(async move {
                    match app.lock().await.get_tree() {
                        Some(tree) => {
                            let _ = tx.send(Msg::OpenSessionTree(tree).into()).await;
                        }
                        None => {
                            let _ = tx
                                .send(Msg::SetStatus("No messages yet".to_string()).into())
                                .await;
                        }
                    }
                });
            }
            KnownCommand::Debug => {
                let _ = self.msg_tx.try_send(Msg::StartStream.into());
                self.spawn_mock_chat();
            }
            KnownCommand::Usage => {
                let tx = self.msg_tx.clone();
                let app = self.app.clone();
                self.rt.spawn(async move {
                    let guard = app.lock().await;
                    match guard.context_window() {
                        Some(cw) => {
                            let consumption = guard.total_consumption().clone();
                            drop(guard);
                            let _ = tx.send(Msg::OpenUsageState(cw, consumption).into()).await;
                        }
                        None => {
                            drop(guard);
                            let _ = tx
                                .send(
                                    Msg::SetStatus(
                                        "no context window data yet — send a message first"
                                            .to_string(),
                                    )
                                    .into(),
                                )
                                .await;
                        }
                    }
                });
            }
        }
    }
}

/// Internal channel carrier: either a model-update message or a side-effect command.
enum Event {
    Msg(Msg),
    Cmd(Cmd),
}

impl From<Msg> for Event {
    fn from(m: Msg) -> Self {
        Event::Msg(m)
    }
}

impl From<Cmd> for Event {
    fn from(c: Cmd) -> Self {
        Event::Cmd(c)
    }
}
