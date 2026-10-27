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
        /// Username, case-insensitive.
        username: String,
        /// New role.
        role: Role,
    },
    /// Revoke sessions. Every session is revoked unless --user is given.
    RevokeAllSessions {
        /// Only this user's sessions.
        #[arg(long)]
        user: Option<String>,
    },
    /// Create demo users, discussions and messages. Refused when YAPSPACE_ENV=prod.
    SeedDev,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Role {
    User,
    Moderator,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Moderator => "moderator",
            Self::Admin => "admin",
        }
    }
}
