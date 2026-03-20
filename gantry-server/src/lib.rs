pub mod app;
pub mod transport;

use anyhow::Result;
use app::llm_port::OllamaLlmAdapter;
use app::service::AppService;
use std::sync::Arc;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;

pub async fn run_from_env() -> Result<()> {
    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);

    run_server(&addr, port).await
}

pub async fn run_server(addr: &str, port: u16) -> Result<()> {
    println!("Starting Gantry server...");
    println!("WS RPC Address: {}:{}", addr, port);

    let llm = Arc::new(OllamaLlmAdapter::new().await?);
    let app = AppService::new(llm);

    let rpc_handle = transport::rpc::start_rpc_server(addr, port, app.clone()).await?;

    println!("Server ready. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;

    println!("Shutting down...");
    rpc_handle.stop()?;
    Ok(())
}
