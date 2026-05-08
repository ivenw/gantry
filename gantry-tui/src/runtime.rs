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
}

impl Runtime {
    /// Creates a new runtime, loading existing messages from the app.
    pub fn new(app: Arc<Mutex<App>>) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let (msg_tx, msg_rx) = channel::<Msg>(256);

        let mut model = Model::new();
        model.session_id = Some(gantry_core::SessionId::new());

        let (existing_messages, selection) = {
            let app = rt.block_on(app.lock());
            (
                ChatMessage::messages_from(app.history()),
                app.selection().cloned(),
            )
        };
        model.chat.messages = existing_messages;
        model.selection = selection;

        Ok(Self {
            model,
            rt,
            msg_tx,
            msg_rx,
            app,
            view_state: ViewState::default(),
            stream_task: None,
            is_streaming: Arc::new(AtomicBool::new(false)),
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
        let app = self.app.clone();
        let is_streaming = self.is_streaming.clone();

        let task = self.rt.spawn(async move {
            match App::stream_message(app, input).await {
                Err(e) => {
                    let _ = tx.send(Msg::StreamResult(Err(e.to_string()))).await;
                }
                Ok(mut stream) => {
                    is_streaming.store(true, Ordering::SeqCst);
                    while let Some(item) = stream.next().await {
                        if tx.send(Msg::StreamItem(item)).await.is_err() {
                            break;
                        }
                    }
                    is_streaming.store(false, Ordering::SeqCst);
                    let _ = tx.send(Msg::StreamDone).await;
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

    fn execute_command(&mut self, cmd: std::sync::Arc<dyn crate::commands::Command>) {
        let ctx = crate::commands::CommandContext {
            app: self.app.clone(),
            msg_tx: self.msg_tx.clone(),
            rt_handle: self.rt.handle().clone(),
        };
        cmd.execute(ctx);
    }
}

