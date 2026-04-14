use anyhow::Result;
use gantry_core::{
    AppService, ConfiguredModel, ModelId, OllamaProviderConfig, ProviderConfig,
    ProviderConfigCatalog, ProviderId, RigAgentFactory,
};
use gantry_rpc::server::start_app_rpc_server;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3444;
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_OLLAMA_MODEL: &str = "ministral-3:3b";

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

    let agent_factory = RigAgentFactory::new(catalog)?;
    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let registry_path = home.join(".gantry").join("projects.json");
    let app = AppService::new(agent_factory, registry_path);

    let rpc_handle = start_app_rpc_server(addr, port, app.clone()).await?;

    println!("Server ready. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;

    println!("Shutting down...");
    rpc_handle.stop()?;
    Ok(())
}
