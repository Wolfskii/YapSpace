//! Command-line interface for the `yapspace` binary.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// YapSpace server and admin tool.
#[derive(Debug, Parser)]
#[command(name = "yapspace", version, about = "YapSpace server and admin tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP and WebSocket server.
    Serve,
    /// Apply pending database migrations, then exit.
    Migrate,
    /// Print the OpenAPI document to a file.
    Openapi {
        /// Output file path, for example docs/api/openapi.json.
        #[arg(long)]
        out: PathBuf,
    },
    /// Check the running server's /healthz endpoint, then exit (for container healthchecks).
    Healthcheck,
    /// Administrative commands.
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum AdminCommand {
    /// Change a user's role.
    SetRole {
        /// Username (lowercase).
        username: String,
        /// New role.
        role: Role,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Role {
    User,
    Moderator,
    Admin,
}
