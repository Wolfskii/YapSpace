//! Entry point for the `yapspace` binary. Subcommands are stubs until their tasks are DONE.

use clap::Parser;
use yapspace_server::cli::{AdminCommand, Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let name = match &cli.command {
        Command::Serve => "serve",
        Command::Migrate => "migrate",
        Command::Openapi { .. } => "openapi",
        Command::Healthcheck => "healthcheck",
        Command::Admin {
            command: AdminCommand::SetRole { .. },
        } => "admin set-role",
    };
    eprintln!("yapspace {name}: not implemented yet");
    std::process::exit(1);
}
