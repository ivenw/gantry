use anyhow::Result;
use gantry_proto::GantryRpcServer;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;
const DEFAULT_SSE_PORT: u16 = 3445;

#[tokio::main]
async fn main() -> Result<()> {
    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);
    let sse_port: u16 = std::env::var("GANTRY_SSE_PORT")
        .unwrap_or_else(|_| DEFAULT_SSE_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_SSE_PORT);

    println!("Starting Gantry server...");
    println!("RPC Address: {}:{}", addr, port);
    println!("SSE Address: {}:{}", addr, sse_port);

    let (handle, sse_handle) = GantryRpcServer::start(addr, port, sse_port).await?;

    println!("Server ready. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;

    println!("Shutting down...");
    handle.stop()?;
    sse_handle.abort();
    Ok(())
}
