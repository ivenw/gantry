use anyhow::Result;
use crossterm::event::{Event, KeyEventKind, MouseEventKind};
use gantry_core::{AppEvent, ChatService, SessionHandle, SessionManager, StreamEvent};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::oneshot;
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
    handle: Arc<SessionHandle>,
    chat_service: Arc<ChatService>,
    session_manager: Arc<SessionManager>,
    project_path: PathBuf,
    view_state: ViewState,
    stream_task: Option<JoinHandle<()>>,
    is_streaming: Arc<AtomicBool>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

impl Runtime {
    pub fn new(
        handle: Arc<SessionHandle>,
        chat_service: Arc<ChatService>,
        session_manager: Arc<SessionManager>,
        project_path: PathBuf,
    ) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let (msg_tx, msg_rx) = channel::<Msg>(256);

        let mut model = Model::new();
        model.session_id = Some({
            // Derive a display session id from the handle's messages count — we use a fixed
            // placeholder since the handle doesn't expose a session id directly. The model uses
            // session_id only for display, so a stable value is fine.
            gantry_core::SessionId::new()
        });

        // Load existing messages from the session handle.
        let existing_messages: Vec<ChatMessage> = {
            let msgs = rt.block_on(handle.get_messages());
            ChatMessage::messages_from_rig(msgs)
        };
        model.chat.messages = existing_messages;

        Ok(Self {
            model,
            rt,
            msg_tx,
            msg_rx,
            handle,
            chat_service,
            session_manager,
            project_path,
            view_state: ViewState::default(),
            stream_task: None,
            is_streaming: Arc::new(AtomicBool::new(false)),
            cancel_tx: None,
        })
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        terminal.draw(|f| views::render(f, &mut self.model, &mut self.view_state))?;

        let tick_interval = Duration::from_millis(100);
        let mut last_tick = Instant::now();

        loop {
            if crossterm::event::poll(Duration::from_millis(10))? {
                match crossterm::event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
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
        let chained = self.process(msg);
        if let Some(chained) = chained {
            self.process(chained);
        }
        is_quit
    }

    fn process(&mut self, msg: Msg) -> Option<Msg> {
        match msg {
            Msg::NewSession(ref session_id) => {
                // Swap handle to the new session.
                let new_handle = self.rt.block_on(self.session_manager.get_or_load(
                    &self.project_path,
                    session_id,
                    self.rt.block_on(self.handle.get_active_selection()),
                ));
                if let Ok(h) = new_handle {
                    self.handle = h;
                }
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
            Msg::SendMessage(ref input) => {
                self.spawn_send_message(input.clone());
            }
            _ => {}
        }
        update(&mut self.model, &self.view_state, msg)
    }

    fn spawn_send_message(&mut self, input: String) {
        if let Some(old) = self.stream_task.take() {
            old.abort();
        }

        let tx = self.msg_tx.clone();
        let handle = self.handle.clone();
        let chat_service = self.chat_service.clone();
        let is_streaming = self.is_streaming.clone();

        let task = self.rt.spawn(async move {
            let req = gantry_core::StreamMessageRequest { content: input };
            let result = chat_service.stream_message(handle, req).await;
            match result {
                Err(e) => {
                    let _ = tx.send(Msg::StreamResult(Err(e.to_string()))).await;
                }
                Ok((_, cancel_tx, mut event_rx)) => {
                    // The cancel_tx is dropped here — interrupt is handled via is_streaming flag
                    // and the oneshot stored on Runtime. We discard it because Runtime
                    // spawns its own cancel mechanism via interrupt_stream().
                    drop(cancel_tx);
                    is_streaming.store(true, Ordering::SeqCst);
                    while let Some(ev) = event_rx.recv().await {
                        let app_event = stream_event_to_app_event(ev);
                        if let Some(app_event) = app_event {
                            if tx.send(Msg::AppEvent(app_event)).await.is_err() {
                                break;
                            }
                        }
                    }
                    is_streaming.store(false, Ordering::SeqCst);
                    let _ = tx.send(Msg::StreamResult(Ok(()))).await;
                }
            }
        });
        self.stream_task = Some(task);
    }

    fn interrupt_stream(&mut self) {
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
        self.is_streaming.store(false, Ordering::SeqCst);
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }

    fn spawn_branch(&mut self, entry_id: String) {
        let tx = self.msg_tx.clone();
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            if let Err(e) = handle.branch(entry_id).await {
                let _ = tx
                    .send(Msg::SetStatus(format!("branch failed: {}", e)))
                    .await;
                return;
            }
            let messages = ChatMessage::messages_from_rig(handle.get_messages().await);
            let _ = tx.send(Msg::ReloadMessages(messages)).await;
        });
    }

    fn spawn_branch_with_input(&mut self, branch_id: String, input: String) {
        let tx = self.msg_tx.clone();
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            if let Err(e) = handle.branch(branch_id).await {
                let _ = tx
                    .send(Msg::SetStatus(format!("branch failed: {}", e)))
                    .await;
                return;
            }
            let messages = ChatMessage::messages_from_rig(handle.get_messages().await);
            let _ = tx
                .send(Msg::ReloadMessagesWithInput(messages, input))
                .await;
        });
    }

    fn execute_command(&mut self, cmd: std::sync::Arc<dyn crate::commands::Command>) {
        let ctx = crate::commands::CommandContext {
            handle: self.handle.clone(),
            chat_service: self.chat_service.clone(),
            session_manager: self.session_manager.clone(),
            project_path: self.project_path.clone(),
            msg_tx: self.msg_tx.clone(),
            rt_handle: self.rt.handle().clone(),
        };
        cmd.execute(ctx);
    }
}

/// Converts a domain [`StreamEvent`] into an [`AppEvent`] for the TUI model.
fn stream_event_to_app_event(ev: StreamEvent) -> Option<AppEvent> {
    use gantry_core::{
        ErrorEvent, MessageReceivedEvent, PendingClearedEvent, StreamEndEvent, StreamStartEvent,
        TokenEvent, ToolCallStartedEvent, ToolResultReceivedEvent,
    };
    let app_event = match ev {
        StreamEvent::MessageReceived { content, pending_id } => {
            AppEvent::MessageReceived(MessageReceivedEvent {
                id: pending_id,
                content,
            })
        }
        StreamEvent::StreamStart {
            message_id,
            pending_id,
        } => AppEvent::StreamStart(StreamStartEvent {
            message_id,
            pending_of: pending_id,
        }),
        StreamEvent::Token { message_id, delta } => {
            AppEvent::Token(TokenEvent { message_id, delta })
        }
        StreamEvent::StreamEnd {
            message_id,
            content,
        } => AppEvent::StreamEnd(StreamEndEvent {
            message_id,
            content,
        }),
        StreamEvent::PendingCleared { pending_id } => {
            AppEvent::PendingCleared(PendingClearedEvent { pending_id })
        }
        StreamEvent::ToolCallStarted {
            tool_call_id,
            tool_name,
        } => AppEvent::ToolCallStarted(ToolCallStartedEvent {
            tool_call_id,
            tool_name,
        }),
        StreamEvent::ToolResultReceived {
            tool_call_id,
            tool_name,
            content,
        } => AppEvent::ToolResultReceived(ToolResultReceivedEvent {
            tool_call_id,
            tool_name,
            content,
        }),
        StreamEvent::Error { message } => AppEvent::Error(ErrorEvent { message }),
    };
    Some(app_event)
}

