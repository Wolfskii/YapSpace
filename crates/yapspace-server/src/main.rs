//! Entry point for the `yapspace` binary.

use std::path::Path;

use clap::Parser;
use yapspace_server::{
    admin,
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
        Command::Admin { command } => admin(command).await,
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

async fn admin(command: AdminCommand) -> anyhow::Result<()> {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("yapspace admin: {err}");
            std::process::exit(2);
        }
    };
    let pool = db::pool(&config)?;
    match command {
        AdminCommand::SetRole { username, role } => {
            admin::set_role(&pool, &username, role.as_str()).await?;
            println!("{username} is now {}", role.as_str());
        }
        AdminCommand::RevokeAllSessions { user } => {
            let revoked = admin::revoke_all_sessions(&pool, user.as_deref()).await?;
            println!("revoked {revoked} session(s)");
        }
        AdminCommand::SeedDev => {
            let hasher = yapspace_server::domain::passwords::PasswordHasher::new();
            admin::seed_dev(&pool, &config, &hasher).await?;
            println!("seeded: alice, bob and mod (password password123!), three discussions");
        }
    }
    pool.close().await;
    Ok(())
}
