use anyhow::{Result, anyhow};

fn main() -> Result<()> {
    let subcommand = std::env::args().nth(1);

    match subcommand.as_deref() {
        None => gantry_tui::run(),
        Some("server") => run_server_command(),
        Some("init") => run_init_command(),
        Some("prune") => run_prune_command(),
        Some(other) => Err(anyhow!("unknown command: {}", other)),
    }
}

fn run_server_command() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(gantry_server::run_from_env())
}

fn run_init_command() -> Result<()> {
    // Optional --path argument, defaults to current directory
    let path = std::env::args()
        .skip(2)
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--path")
        .map(|w| std::path::PathBuf::from(&w[1]))
        .unwrap_or_else(|| std::env::current_dir().expect("cannot determine current directory"));

    let abs = path
        .canonicalize()
        .map_err(|_| anyhow!("path does not exist: {}", path.display()))?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port: u16 = std::env::var("GANTRY_PORT")
            .unwrap_or_else(|_| "3444".to_string())
            .parse()
            .unwrap_or(3444);

        let client = gantry_rpc::JsonRpcClient::connect_ws(&addr, port)
            .await
            .map_err(|e| anyhow!("failed to connect to server: {}", e))?;

        println!("Project registered: {}", abs.display());
        client
            .register_project(abs)
            .await
            .map_err(|e| anyhow!("failed to register project: {}", e))?;
        Ok(())
    })
}

fn run_prune_command() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let addr = std::env::var("GANTRY_ADDR").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port: u16 = std::env::var("GANTRY_PORT")
            .unwrap_or_else(|_| "3444".to_string())
            .parse()
            .unwrap_or(3444);

        let client = gantry_rpc::JsonRpcClient::connect_ws(&addr, port)
            .await
            .map_err(|e| anyhow!("failed to connect to server: {}", e))?;

        let projects = client
            .list_projects()
            .await
            .map_err(|e| anyhow!("failed to list projects: {}", e))?;

        let mut pruned = 0;
        for project in &projects {
            let stale = !project.exists() || !project.join(".gantry").exists();
            if stale {
                client
                    .unregister_project(project.clone())
                    .await
                    .map_err(|e| anyhow!("failed to unregister {}: {}", project.display(), e))?;
                println!("pruned: {}", project.display());
                pruned += 1;
            }
        }

        if pruned == 0 {
            println!("no stale projects found");
        } else {
            println!("{} project(s) pruned", pruned);
        }
        Ok(())
    })
}
