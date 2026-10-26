//! Entry point for the `yapspace` binary.

use clap::Parser;
use yapspace_server::{
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
        Command::Openapi { .. } => not_implemented("openapi"),
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

fn not_implemented(name: &str) -> anyhow::Result<()> {
    eprintln!("yapspace {name}: not implemented yet");
    std::process::exit(1);
}
