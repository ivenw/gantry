use gantry_rpc::{JsonRpcClient, WsConnectionEvent};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

pub struct ReconnectSuccess {
    pub client: JsonRpcClient,
    pub session_id: String,
    pub event_handle: JoinHandle<()>,
    pub event_rx: Receiver<WsConnectionEvent>,
    pub clear_messages: bool,
}

pub struct Connection {
    pub client: Option<Arc<JsonRpcClient>>,
    pub session_id: Arc<Mutex<String>>,
    pub event_handle: JoinHandle<()>,
    pub event_rx: Receiver<WsConnectionEvent>,
    pub reconnect_pending: bool,
    pub addr: String,
    pub port: u16,
    pub project_path: PathBuf,
}

impl Connection {
    pub async fn try_connect_async(
        addr: &str,
        port: u16,
        project_path: &Path,
    ) -> Option<(JsonRpcClient, String, JoinHandle<()>, Receiver<WsConnectionEvent>)> {
        let client = JsonRpcClient::connect_ws(addr, port).await.ok()?;

        let sessions = client.list_sessions(project_path.to_path_buf()).await.ok()?;
        let session_id = if let Some(last) = sessions.last() {
            last.id.clone()
        } else {
            client.create_session(project_path.to_path_buf()).await.ok()?
        };

        client
            .bind_session(session_id.clone(), project_path.to_path_buf())
            .await
            .ok()?;

        let (handle, rx) = client.subscribe_events().await.ok()?;

        Some((client, session_id, handle, rx))
    }

    pub fn new_disconnected(rt: &Runtime, addr: String, port: u16, project_path: PathBuf) -> Self {
        let (_, rx) = tokio::sync::mpsc::channel(1);
        let handle = rt.spawn(async {});
        Self {
            client: None,
            session_id: Arc::new(Mutex::new(String::new())),
            event_handle: handle,
            event_rx: rx,
            reconnect_pending: false,
            addr,
            port,
            project_path,
        }
    }

    pub fn new_connected(
        client: JsonRpcClient,
        session_id: String,
        event_handle: JoinHandle<()>,
        event_rx: Receiver<WsConnectionEvent>,
        addr: String,
        port: u16,
        project_path: PathBuf,
    ) -> Self {
        Self {
            client: Some(Arc::new(client)),
            session_id: Arc::new(Mutex::new(session_id)),
            event_handle,
            event_rx,
            reconnect_pending: false,
            addr,
            port,
            project_path,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub fn spawn_reconnect(&self, rt: &Runtime, tx: mpsc::SyncSender<ReconnectSuccess>) {
        let addr = self.addr.clone();
        let port = self.port;
        let project_path = self.project_path.clone();
        rt.spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if let Some((client, session_id, event_handle, event_rx)) =
                    Self::try_connect_async(&addr, port, &project_path).await
                {
                    let _ = tx.send(ReconnectSuccess {
                        client,
                        session_id,
                        event_handle,
                        event_rx,
                        clear_messages: false,
                    });
                    return;
                }
            }
        });
    }

    pub fn on_disconnected(&mut self, rt: &Runtime, tx: &mpsc::SyncSender<ReconnectSuccess>) {
        self.client = None;
        if !self.reconnect_pending {
            self.reconnect_pending = true;
            self.spawn_reconnect(rt, tx.clone());
        }
    }

    pub fn on_reconnect_success(&mut self, success: ReconnectSuccess) {
        self.event_handle.abort();
        self.event_handle = success.event_handle;
        self.event_rx = success.event_rx;
        *self.session_id.lock().unwrap() = success.session_id;
        self.client = Some(Arc::new(success.client));
        self.reconnect_pending = false;
    }

    pub fn is_connection_error(err: &str) -> bool {
        err.contains("connection refused")
            || err.contains("failed to connect")
            || err.contains("broken pipe")
            || err.contains("WebSocket")
            || err.contains("os error")
    }
}
