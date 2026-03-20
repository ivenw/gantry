use anyhow::{Result, anyhow};

fn main() -> Result<()> {
    let subcommand = std::env::args().nth(1);

    match subcommand.as_deref() {
        None => gantry_tui::run(),
        Some("server") => run_server_command(),
        Some(other) => Err(anyhow!("unknown command: {}", other)),
    }
}

fn run_server_command() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(gantry_server::run_from_env())
}
