use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    gantry_server::run_from_env().await
}
