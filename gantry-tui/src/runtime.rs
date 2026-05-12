use anyhow::Result;
use crossterm::event::{Event, KeyEventKind, MouseEventKind};
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

use crate::message::Msg;
use crate::model::{ChatMessage, Model};
use crate::update::update;
use crate::views::{self, ViewState};

pub struct Runtime {
    model: Model,
    rt: tokio::runtime::Runtime,
    msg_tx: Sender<Msg>,
    msg_rx: Receiver<Msg>,
    app: Arc<Mutex<App>>,
    view_state: ViewState,
    stream_task: Option<JoinHandle<()>>,
    is_streaming: Arc<AtomicBool>,
    cancel_stream: Arc<AtomicBool>,
}

impl Runtime {
    /// Creates a new runtime, loading existing messages from the app.
    pub fn new(app: Arc<Mutex<App>>, cwd: std::path::PathBuf) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let (msg_tx, msg_rx) = channel::<Msg>(256);

        let mut model = Model::new();

        let (session_id, existing_messages, selection, project_path) = {
            let app = rt.block_on(app.lock());
            (
                app.session_id().clone(),
                ChatMessage::messages_from(app.history()),
                app.selection().cloned(),
                app.project_path.clone(),
            )
        };
        model.session_id = Some(session_id);
        model.chat.messages = existing_messages;
        model.selection = selection;
        model.project_path = project_path;
        model.cwd = cwd;

        Ok(Self {
            model,
            rt,
            msg_tx,
            msg_rx,
            app,
            view_state: ViewState::default(),
            stream_task: None,
            is_streaming: Arc::new(AtomicBool::new(false)),
            cancel_stream: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        terminal.draw(|f| views::render(f, &mut self.model, &mut self.view_state))?;

        let tick_interval = Duration::from_millis(100);
        let mut last_tick = Instant::now();

        loop {
            if crossterm::event::poll(Duration::from_millis(10))? {
                match crossterm::event::read()? {
                    Event::Key(key)
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
                    {
                        let _ = self.msg_tx.try_send(Msg::Key(key));
                    }
                    Event::Mouse(mouse) => {
                        let delta: i32 = match mouse.kind {
                            MouseEventKind::ScrollUp => 1,
                            MouseEventKind::ScrollDown => -1,
                            _ => 0,
                        };
                        if delta != 0 {
                            let _ = self.msg_tx.try_send(Msg::ScrollChat(delta));
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
                self.view_state.statusline.tick();
                needs_redraw = true;
            }

            if needs_redraw {
                terminal.draw(|f| views::render(f, &mut self.model, &mut self.view_state))?;
            }
        }
    }

    fn dispatch(&mut self, msg: Msg) -> bool {
        let is_quit = matches!(msg, Msg::Quit);
        let mut next = self.process(msg);
        while let Some(msg) = next {
            next = self.process(msg);
        }
        is_quit
    }

    fn process(&mut self, msg: Msg) -> Option<Msg> {
        match msg {
            Msg::NewSession => {
                let result = self
                    .rt
                    .block_on(async { self.app.lock().await.new_session() });
                if let Err(e) = result {
                    return Some(Msg::SetStatus(format!("failed to create session: {}", e)));
                }
                self.model.chat.messages.clear();
                return None;
            }
            Msg::RunCommand(cmd) => {
                self.run_command(cmd);
                return None;
            }
            Msg::InterruptStream => {
                self.interrupt_stream();
                self.model.chat.finish_streaming();
                return None;
            }
            Msg::BranchTo(ref entry_id) => {
                self.spawn_branch(entry_id.clone());
                return None;
            }
            Msg::BranchToWithInput {
                ref branch_id,
                ref input,
            } => {
                self.spawn_branch_with_input(branch_id.clone(), input.clone());
                return None;
            }
            Msg::SendMessage(ref tokens) => {
                self.spawn_send_message(tokens.clone());
            }
            Msg::OpenPathPicker(ref query) => {
                let paths = self
                    .rt
                    .block_on(async { self.app.lock().await.search_paths(query) });
                self.model.activate_path_picker(paths);
                return None;
            }
            Msg::OpenSkillPicker(ref query) => {
                let skills = self
                    .rt
                    .block_on(async { self.app.lock().await.search_skills(query) });
                self.model.activate_skill_picker(skills);
                return None;
            }
            Msg::RefineAttachmentPicker(ref query) => {
                let is_skill = matches!(
                    self.model.attachment_picker.as_ref().map(|p| &p.kind),
                    Some(crate::model::AttachmentPickerKind::Skill(_))
                );
                if is_skill {
                    let skills = self
                        .rt
                        .block_on(async { self.app.lock().await.search_skills(query) });
                    return Some(Msg::SetSkillPickerResults(skills));
                } else {
                    let paths = self
                        .rt
                        .block_on(async { self.app.lock().await.search_paths(query) });
                    return Some(Msg::SetPathPickerResults(paths));
                }
            }
            Msg::AddProvider(ref config, ref credential) => {
                self.handle_add_provider(config.clone(), credential.clone());
                return None;
            }
            Msg::RemoveProvider(ref alias) => {
                self.handle_remove_provider(alias.clone());
                return None;
            }
            Msg::OpenModelPicker(ref models) => {
                self.model.cached_models = Some(models.clone());
            }
            Msg::SelectModel(ref selection) => {
                self.handle_select_model(selection.clone());
                return None;
            }
            Msg::ResumeSession(ref session_id) => {
                self.handle_resume_session(session_id.clone());
                return None;
            }
            _ => {}
        }
        update(&mut self.model, &self.view_state, msg)
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
                    let _ = tx.send(Msg::StreamError(e.to_string())).await;
                }
                Ok(mut response) => {
                    is_streaming.store(true, Ordering::SeqCst);
                    while let Some(item) = response.stream.next().await {
                        if cancel.load(Ordering::SeqCst)
                            || tx.send(Msg::StreamItem(item)).await.is_err()
                        {
                            break;
                        }
                    }
                    response.commit().await;
                    if let Some(cw) = app_ref.lock().await.context_window() {
                        let _ = tx.send(Msg::ContextWindowUpdated(cw)).await;
                    }
                    is_streaming.store(false, Ordering::SeqCst);
                    let _ = tx.send(Msg::StreamDone).await;
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

        let task = self.rt.spawn(async move {
            let mut response = gantry_core::app::mock_chat();
            is_streaming.store(true, Ordering::SeqCst);
            while let Some(item) = response.stream.next().await {
                if cancel.load(Ordering::SeqCst)
                    || tx.send(Msg::StreamItem(item)).await.is_err()
                {
                    break;
                }
            }
            response.commit().await;
            is_streaming.store(false, Ordering::SeqCst);
            let _ = tx.send(Msg::StreamDone).await;
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
                    .send(Msg::SetStatus(format!("branch failed: {}", e)))
                    .await;
                return;
            }
            let messages = ChatMessage::messages_from(app.lock().await.history());
            let _ = tx.send(Msg::ReloadMessages(messages)).await;
        });
    }

    fn spawn_branch_with_input(&mut self, branch_id: String, input: String) {
        let tx = self.msg_tx.clone();
        let app = self.app.clone();
        self.rt.spawn(async move {
            if let Err(e) = app.lock().await.branch(&branch_id) {
                let _ = tx
                    .send(Msg::SetStatus(format!("branch failed: {}", e)))
                    .await;
                return;
            }
            let messages = ChatMessage::messages_from(app.lock().await.history());
            let _ = tx.send(Msg::ReloadMessagesWithInput(messages, input)).await;
        });
    }

    fn handle_add_provider(
        &mut self,
        config: gantry_core::ProviderConfig,
        credential: Option<gantry_core::StoredCredential>,
    ) {
        match self
            .rt
            .block_on(async { self.app.lock().await.add_provider(config, credential) })
        {
            Ok(()) => {
                self.model.cached_models = None;
                let providers = self
                    .rt
                    .block_on(async { self.app.lock().await.list_providers().to_vec() });
                self.model.activate_providers_view(providers);
            }
            Err(e) => {
                // Surface the error inside the wizard.
                if let Some(ref mut pv) = self.model.providers_view
                    && let crate::model::ProvidersSubView::Wizard(ref mut w) = pv.sub
                {
                    w.error = Some(e.to_string());
                } else {
                    self.model.status_message = Some(e.to_string());
                }
            }
        }
    }

    fn handle_remove_provider(&mut self, alias: gantry_core::ProviderAlias) {
        match self
            .rt
            .block_on(async { self.app.lock().await.remove_provider(&alias) })
        {
            Ok(()) => {
                self.model.cached_models = None;
                let providers = self
                    .rt
                    .block_on(async { self.app.lock().await.list_providers().to_vec() });
                // Refresh the list view, clamping selection if it is now out of bounds.
                if let Some(ref mut pv) = self.model.providers_view
                    && let crate::model::ProvidersSubView::List {
                        ref mut selected_idx,
                    } = pv.sub
                {
                    pv.providers = providers;
                    if !pv.providers.is_empty() {
                        *selected_idx = (*selected_idx).min(pv.providers.len() - 1);
                    } else {
                        *selected_idx = 0;
                    }
                }
            }
            Err(e) => {
                self.model.status_message = Some(e.to_string());
            }
        }
    }

    fn handle_resume_session(&mut self, session_id: gantry_core::SessionId) {
        let result = self
            .rt
            .block_on(async { self.app.lock().await.resume_session(&session_id) });
        match result {
            Err(e) => {
                self.model.status_message = Some(format!("failed to resume session: {e}"));
            }
            Ok(()) => {
                let messages = self.rt.block_on(async {
                    ChatMessage::messages_from(self.app.lock().await.history())
                });
                self.model.chat.messages = messages;
                self.model.chat.scroll_offset = 0;
                self.model.chat.user_is_scrolling = false;
                self.model.session_id = Some(session_id);
            }
        }
    }

    fn handle_select_model(&mut self, selection: gantry_core::ModelSelection) {
        self.rt.block_on(async {
            self.app.lock().await.set_selection(selection.clone());
        });
        self.model.selection = Some(selection);
    }

    /// Dispatches a `KnownCommand`, either immediately updating the model or spawning an async task.
    fn run_command(&mut self, cmd: crate::commands::KnownCommand) {
        use crate::commands::KnownCommand;
        match cmd {
            KnownCommand::Quit => {
                let _ = self.msg_tx.try_send(Msg::Quit);
            }
            KnownCommand::New => {
                let _ = self.msg_tx.try_send(Msg::NewSession);
            }
            KnownCommand::Model => {
                if let Some(models) = self.model.cached_models.clone() {
                    self.model.activate_model_picker_view(models);
                    return;
                }
                let tx = self.msg_tx.clone();
                let app = self.app.clone();
                self.rt.spawn(async move {
                    match app.lock().await.list_models().await {
                        Ok(models) => {
                            let _ = tx.send(Msg::OpenModelPicker(models)).await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Msg::SetStatus(format!("Failed to list models: {e}")))
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
                    let _ = tx.send(Msg::OpenProvidersView(providers)).await;
                });
            }
            KnownCommand::Sessions => {
                let tx = self.msg_tx.clone();
                let app = self.app.clone();
                self.rt.spawn(async move {
                    let app = app.lock().await;
                    match app.list_sessions() {
                        Ok(sessions) => {
                            let active_id = app.session_id().clone();
                            let _ = tx.send(Msg::OpenSessionsView(sessions, active_id)).await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Msg::SetStatus(format!("failed to list sessions: {e}")))
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
                            let _ = tx.send(Msg::OpenTreeView(tree)).await;
                        }
                        None => {
                            let _ = tx.send(Msg::SetStatus("No messages yet".into())).await;
                        }
                    }
                });
            }
            KnownCommand::Debug => {
                self.model.chat.start_streaming_message();
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
                            let _ = tx.send(Msg::OpenUsageView(cw, consumption)).await;
                        }
                        None => {
                            drop(guard);
                            let _ = tx
                                .send(Msg::SetStatus(
                                    "no context window data yet — send a message first"
                                        .to_string(),
                                ))
                                .await;
                        }
                    }
                });
            }
        }
    }
}
