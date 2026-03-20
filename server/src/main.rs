use anyhow::Result;
use gantry_proto::server::GantryRpcServer;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;

#[tokio::main]
async fn main() -> Result<()> {
    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);

    println!("Starting Gantry server...");
    println!("Address: {}:{}", addr, port);

    let handle = GantryRpcServer::start(&addr, port).await?;

    println!("Server ready. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;

    println!("Shutting down...");
    handle.stop()?;
    Ok(())
}
