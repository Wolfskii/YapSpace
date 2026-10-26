//! Entry point for the `yapspace` binary.

use std::path::Path;

use clap::Parser;
use yapspace_server::{
    api::openapi as api_openapi,
    app::{self, AppState},
    cli::{AdminCommand, Cli, Command},
    config::Config,
    db, telemetry,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve => serve().await,
        Command::Healthcheck => healthcheck().await,
        Command::Migrate => migrate().await,
        Command::Openapi { out } => openapi(&out),
        Command::Admin {
            command: AdminCommand::SetRole { .. },
        } => not_implemented("admin set-role"),
    }
}

async fn serve() -> anyhow::Result<()> {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("yapspace serve: {err}");
            std::process::exit(2);
        }
    };
    telemetry::init_tracing(&config);
    telemetry::init_metrics(config.metrics_addr)?;
    let state = AppState::new(config)?;
    app::serve(state).await
}

async fn migrate() -> anyhow::Result<()> {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("yapspace migrate: {err}");
            std::process::exit(2);
        }
    };
    let pool = db::pool(&config)?;
    db::migrate(&pool).await?;
    pool.close().await;
    println!("migrations complete");
    Ok(())
}

async fn healthcheck() -> anyhow::Result<()> {
    // Only the bind address matters here, so the database URL is not required.
    let bind = std::env::var("YAPSPACE_BIND_ADDR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "0.0.0.0:8080".into());
    let bind = bind.trim().parse()?;
    app::run_healthcheck(bind).await
}

fn openapi(out: &Path) -> anyhow::Result<()> {
    let json = api_openapi::to_pretty_json()?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, json)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn not_implemented(name: &str) -> anyhow::Result<()> {
    eprintln!("yapspace {name}: not implemented yet");
    std::process::exit(1);
}
