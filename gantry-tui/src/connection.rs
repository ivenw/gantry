use gantry_rpc::{JsonRpcClient, WsConnectionEvent};
use std::path::Path;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

pub async fn try_connect_async(
    addr: &str,
    port: u16,
    project_path: &Path,
) -> Option<(
    JsonRpcClient,
    String,
    JoinHandle<()>,
    Receiver<WsConnectionEvent>,
)> {
    let client = JsonRpcClient::connect_ws(addr, port).await.ok()?;

    let sessions = client
        .list_sessions(project_path.to_path_buf())
        .await
        .ok()?;
    let session_id = if let Some(last) = sessions.last() {
        last.id.clone()
    } else {
        client
            .create_session(project_path.to_path_buf())
            .await
            .ok()?
    };

    client
        .bind_session(session_id.clone(), project_path.to_path_buf())
        .await
        .ok()?;

    let (handle, rx) = client.subscribe_events().await.ok()?;

    Some((client, session_id, handle, rx))
}
