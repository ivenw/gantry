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
    pub fn new(app: Arc<Mutex<App>>) -> Result<Self> {
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
                        if key.kind == KeyEventKind::Press
                            || key.kind == KeyEventKind::Repeat =>
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
                let result = self.rt.block_on(async { self.app.lock().await.new_session() });
                if let Err(e) = result {
                    return Some(Msg::SetStatus(format!("failed to create session: {}", e)));
                }
                self.model.chat.messages.clear();
                return None;
            }
            Msg::ExecuteCommand(ref cmd) => {
                self.execute_command(cmd.clone());
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
                let paths = self.rt.block_on(async {
                    self.app.lock().await.search_paths(query)
                });
                self.model.activate_path_picker(paths);
                return None;
            }
            Msg::OpenSkillPicker(ref query) => {
                let skills = self.rt.block_on(async {
                    self.app.lock().await.search_skills(query)
                });
                self.model.activate_skill_picker(skills);
                return None;
            }
            Msg::RefineAttachmentPicker(ref query) => {
                let is_skill = matches!(
                    self.model.attachment_picker.as_ref().map(|p| &p.kind),
                    Some(crate::model::AttachmentPickerKind::Skill(_))
                );
                if is_skill {
                    let skills = self.rt.block_on(async {
                        self.app.lock().await.search_skills(query)
                    });
                    return Some(Msg::SetSkillPickerResults(skills));
                } else {
                    let paths = self.rt.block_on(async {
                        self.app.lock().await.search_paths(query)
                    });
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
                Ok((mut response, mut hook_rx)) => {
                    is_streaming.store(true, Ordering::SeqCst);
                    let hook_tx = tx.clone();
                    tokio::spawn(async move {
                        while let Some(event) = hook_rx.recv().await {
                            let msg = match event {
                                gantry_core::HookEvent::ToolCallStarted { name, id } => {
                                    Msg::ToolCallStarted { name, id }
                                }
                                gantry_core::HookEvent::ToolCallFinished { id } => {
                                    Msg::ToolCallFinished { id }
                                }
                            };
                            if hook_tx.send(msg).await.is_err() {
                                break;
                            }
                        }
                    });
                    while let Some(item) = response.stream.next().await {
                        if cancel.load(Ordering::SeqCst) || tx.send(Msg::StreamItem(item)).await.is_err() {
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
            let _ = tx
                .send(Msg::ReloadMessagesWithInput(messages, input))
                .await;
        });
    }

    fn handle_add_provider(
        &mut self,
        config: gantry_core::ProviderConfig,
        credential: Option<gantry_core::StoredCredential>,
    ) {
        match self.rt.block_on(async {
            self.app.lock().await.add_provider(config, credential)
        }) {
            Ok(()) => {
                let providers = self.rt.block_on(async {
                    self.app.lock().await.list_providers().to_vec()
                });
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
        match self.rt.block_on(async {
            self.app.lock().await.remove_provider(&alias)
        }) {
            Ok(()) => {
                let providers = self.rt.block_on(async {
                    self.app.lock().await.list_providers().to_vec()
                });
                // Refresh the list view, clamping selection if it is now out of bounds.
                if let Some(ref mut pv) = self.model.providers_view
                    && let crate::model::ProvidersSubView::List { ref mut selected_idx } = pv.sub
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
        let result = self.rt.block_on(async {
            self.app.lock().await.resume_session(&session_id)
        });
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

    fn execute_command(&mut self, cmd: std::sync::Arc<dyn crate::commands::Command>) {
        let ctx = crate::commands::CommandContext {
            app: self.app.clone(),
            msg_tx: self.msg_tx.clone(),
            rt_handle: self.rt.handle().clone(),
        };
        cmd.execute(ctx);
    }
}

