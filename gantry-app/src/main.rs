use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialise a gantry project by writing `gantry.toml`.
    Init {
        /// Directory to initialise; defaults to the current working directory.
        path: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => gantry_tui::run(),
        Some(Command::Init { path }) => init(path),
    }
}

/// Writes a `gantry.toml` into `path` (or the cwd) to initialise a project.
fn init(path: Option<PathBuf>) -> Result<()> {
    let dir = match path {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let config_path = dir.join("gantry.toml");
    gantry_core::ProjectConfig::init(&config_path)?;
    println!("Initialised gantry project at {}", config_path.display());
    Ok(())
}
