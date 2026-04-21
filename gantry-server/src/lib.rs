mod rpc;

use anyhow::Result;
use gantry_core::fs::FsProjectRegistry;
use gantry_core::{
    ConfiguredModel, ModelId, OllamaProviderConfig, ProviderConfig, ProviderConfigCatalog,
    ProviderId, RigAgentFactory, SessionManager, dirs::GlobalConfigDir,
};
use gantry_rpc::GantryRpcServer;
use jsonrpsee::server::{ServerBuilder, ServerConfig};
use rpc::RpcApp;
use std::sync::Arc;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_OLLAMA_MODEL: &str = "ministral-3:3b";

/// Reads server configuration from environment variables and starts the server.
pub async fn run_from_env() -> Result<()> {
    let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let port: u16 = std::env::var("GANTRY_PORT")
        .unwrap_or_else(|_| DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or(DEFAULT_PORT);

    run_server(&addr, port).await
}

/// Builds domain state, starts the JSON-RPC server, and blocks until Ctrl-C.
pub async fn run_server(addr: &str, port: u16) -> Result<()> {
    println!("Starting Gantry server...");
    println!("WS RPC Address: {}:{}", addr, port);

    let ollama_url =
        std::env::var("GANTRY_OLLAMA_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
    let ollama_model =
        std::env::var("GANTRY_OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_OLLAMA_MODEL.to_string());
    let catalog = ProviderConfigCatalog {
        providers: vec![ProviderConfig::Ollama(OllamaProviderConfig {
            id: ProviderId::new("ollama"),
            base_url: ollama_url,
            models: vec![ConfiguredModel {
                id: ModelId::new("default"),
                provider_model_name: ollama_model,
            }],
            default_model: ModelId::new("default"),
        })],
        default_provider: ProviderId::new("ollama"),
    };

    let data_dir = GlobalConfigDir::new()?;
    let projects = Arc::new(FsProjectRegistry::new(&data_dir)?);
    let sessions = Arc::new(SessionManager::new());
    let agent_factory = RigAgentFactory::new(catalog)?;

    let rpc_app = RpcApp::new(projects, sessions, agent_factory);
    let module = rpc_app.into_rpc().remove_context();

    let config = ServerConfig::builder().ws_only().build();
    let rpc_server = ServerBuilder::new()
        .set_config(config)
        .build((addr, port))
        .await?;
    let rpc_handle = rpc_server.start(module);

    println!("Server ready. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;

    println!("Shutting down...");
    rpc_handle.stop()?;
    Ok(())
}
