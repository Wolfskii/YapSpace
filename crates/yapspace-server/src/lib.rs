//! Holds the YapSpace HTTP/WS server, its domain logic, and the admin CLI.

pub mod api;
pub mod app;
pub mod cli;
pub mod client_ip;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod jobs;
pub mod ratelimit;
pub mod realtime;
pub mod spam;
pub mod telemetry;
