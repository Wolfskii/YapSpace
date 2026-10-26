//! Server configuration from `YAPSPACE_*` environment variables (see docs/architecture/deployment.md §J.5).
//!
//! Every variable `X` can also be given as `X_FILE`, which holds a path to a file with the value. `X_FILE` wins
//! over `X`. This keeps secrets out of the process environment.

use std::{fmt, net::SocketAddr, str::FromStr, time::Duration};

use ipnet::IpNet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    Dev,
    Prod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Clone)]
pub struct Config {
    pub env: Env,
    pub database_url: String,
    pub db_max_connections: u32,
    pub bind_addr: SocketAddr,
    pub metrics_addr: SocketAddr,
    pub log_format: LogFormat,
    pub trusted_proxies: Vec<IpNet>,
    pub allowed_ws_origins: Vec<String>,
    pub ws_max_connections: usize,
    pub registration_open: bool,
    pub extra_blocked_hosts: Vec<String>,
    pub shutdown_grace: Duration,
}

// Hand-written so the database URL (which contains the password) never shows up in logs or panics.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("env", &self.env)
            .field("database_url", &"<redacted>")
            .field("db_max_connections", &self.db_max_connections)
            .field("bind_addr", &self.bind_addr)
            .field("metrics_addr", &self.metrics_addr)
            .field("log_format", &self.log_format)
            .field("trusted_proxies", &self.trusted_proxies)
            .field("allowed_ws_origins", &self.allowed_ws_origins)
            .field("ws_max_connections", &self.ws_max_connections)
            .field("registration_open", &self.registration_open)
            .field("extra_blocked_hosts", &self.extra_blocked_hosts)
            .field("shutdown_grace", &self.shutdown_grace)
            .finish()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing {0} (or {0}_FILE)")]
    Missing(&'static str),
    #[error("{name} has an invalid value: {reason}")]
    Invalid { name: &'static str, reason: String },
    #[error("cannot read the file named by {name}_FILE: {reason}")]
    File { name: &'static str, reason: String },
}

impl Config {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds the configuration from any key/value source. Tests use this instead of the process environment.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let env = match value(&get, "YAPSPACE_ENV")? {
            None => Env::Dev,
            Some(v) => match v.as_str() {
                "dev" => Env::Dev,
                "prod" => Env::Prod,
                other => {
                    return Err(invalid(
                        "YAPSPACE_ENV",
                        format!("expected dev or prod, got {other:?}"),
                    ));
                }
            },
        };

        let database_url =
            value(&get, "YAPSPACE_DATABASE_URL")?.ok_or(ConfigError::Missing("YAPSPACE_DATABASE_URL"))?;

        let default_format = match env {
            Env::Dev => LogFormat::Pretty,
            Env::Prod => LogFormat::Json,
        };
        let log_format = match value(&get, "YAPSPACE_LOG_FORMAT")? {
            None => default_format,
            Some(v) => match v.as_str() {
                "pretty" => LogFormat::Pretty,
                "json" => LogFormat::Json,
                other => {
                    return Err(invalid(
                        "YAPSPACE_LOG_FORMAT",
                        format!("expected pretty or json, got {other:?}"),
                    ));
                }
            },
        };

        Ok(Config {
            env,
            database_url,
            db_max_connections: parse_or(&get, "YAPSPACE_DB_MAX_CONNECTIONS", 20u32)?,
            bind_addr: parse_or(
                &get,
                "YAPSPACE_BIND_ADDR",
                "0.0.0.0:8080".parse().expect("valid default"),
            )?,
            metrics_addr: parse_or(
                &get,
                "YAPSPACE_METRICS_ADDR",
                "0.0.0.0:9090".parse().expect("valid default"),
            )?,
            log_format,
            trusted_proxies: list(&get, "YAPSPACE_TRUSTED_PROXIES")?
                .iter()
                .map(|s| {
                    s.parse::<IpNet>()
                        .map_err(|e| invalid("YAPSPACE_TRUSTED_PROXIES", format!("{s:?}: {e}")))
                })
                .collect::<Result<_, _>>()?,
            allowed_ws_origins: list(&get, "YAPSPACE_ALLOWED_WS_ORIGINS")?,
            ws_max_connections: parse_or(&get, "YAPSPACE_WS_MAX_CONNECTIONS", 20_000usize)?,
            registration_open: parse_bool_or(&get, "YAPSPACE_REGISTRATION_OPEN", true)?,
            extra_blocked_hosts: list(&get, "YAPSPACE_EXTRA_BLOCKED_HOSTS")?,
            shutdown_grace: Duration::from_secs(parse_or(&get, "YAPSPACE_SHUTDOWN_GRACE_SECS", 20u64)?),
        })
        .and_then(validate)
    }
}

fn validate(config: Config) -> Result<Config, ConfigError> {
    if config.db_max_connections == 0 {
        return Err(invalid(
            "YAPSPACE_DB_MAX_CONNECTIONS",
            "must be at least 1".into(),
        ));
    }
    if config.ws_max_connections == 0 {
        return Err(invalid(
            "YAPSPACE_WS_MAX_CONNECTIONS",
            "must be at least 1".into(),
        ));
    }
    Ok(config)
}

fn invalid(name: &'static str, reason: String) -> ConfigError {
    ConfigError::Invalid { name, reason }
}

/// Reads `name`, or `name_FILE` if that is set (it wins). Empty values count as unset.
fn value(get: &impl Fn(&str) -> Option<String>, name: &'static str) -> Result<Option<String>, ConfigError> {
    let file_key = format!("{name}_FILE");
    if let Some(path) = get(&file_key) {
        let contents = std::fs::read_to_string(path.trim()).map_err(|e| ConfigError::File {
            name,
            reason: e.to_string(),
        })?;
        return Ok(Some(contents.trim().to_string()).filter(|v| !v.is_empty()));
    }
    Ok(get(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()))
}

fn parse_or<T>(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: T,
) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    match value(get, name)? {
        None => Ok(default),
        Some(v) => v.parse::<T>().map_err(|e| invalid(name, format!("{v:?}: {e}"))),
    }
}

fn parse_bool_or(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: bool,
) -> Result<bool, ConfigError> {
    match value(get, name)? {
        None => Ok(default),
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" => Ok(false),
            other => Err(invalid(name, format!("expected true or false, got {other:?}"))),
        },
    }
}

/// Comma-separated list. Items are trimmed and empty items are dropped.
fn list(get: &impl Fn(&str) -> Option<String>, name: &'static str) -> Result<Vec<String>, ConfigError> {
    Ok(value(get, name)?
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    const DB: (&str, &str) = ("YAPSPACE_DATABASE_URL", "postgres://u:p@h/db");

    #[test]
    fn defaults_with_only_database_url() {
        let c = Config::from_lookup(lookup(&[DB])).unwrap();
        assert_eq!(c.env, Env::Dev);
        assert_eq!(c.log_format, LogFormat::Pretty);
        assert_eq!(c.db_max_connections, 20);
        assert_eq!(c.bind_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(c.metrics_addr, "0.0.0.0:9090".parse().unwrap());
        assert!(c.trusted_proxies.is_empty());
        assert!(c.registration_open);
        assert_eq!(c.shutdown_grace, Duration::from_secs(20));
    }

    #[test]
    fn missing_database_url_names_the_variable() {
        let err = Config::from_lookup(lookup(&[])).unwrap_err();
        assert_eq!(err, ConfigError::Missing("YAPSPACE_DATABASE_URL"));
        assert_eq!(
            err.to_string(),
            "missing YAPSPACE_DATABASE_URL (or YAPSPACE_DATABASE_URL_FILE)"
        );
    }

    #[test]
    fn prod_defaults_to_json_logs() {
        let c = Config::from_lookup(lookup(&[DB, ("YAPSPACE_ENV", "prod")])).unwrap();
        assert_eq!(c.env, Env::Prod);
        assert_eq!(c.log_format, LogFormat::Json);
    }

    #[test]
    fn file_variant_wins_over_plain_variable() {
        let path = std::env::temp_dir().join(format!("yapspace-cfg-{}.txt", uuid::Uuid::now_v7()));
        std::fs::write(&path, "postgres://from-file/db\n").unwrap();
        let file = path.to_string_lossy().to_string();
        let c = Config::from_lookup(lookup(&[
            ("YAPSPACE_DATABASE_URL", "postgres://from-env/db"),
            ("YAPSPACE_DATABASE_URL_FILE", &file),
        ]))
        .unwrap();
        assert_eq!(c.database_url, "postgres://from-file/db");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unreadable_file_is_an_error() {
        let err = Config::from_lookup(lookup(&[("YAPSPACE_DATABASE_URL_FILE", "/definitely/not/here")]))
            .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::File {
                name: "YAPSPACE_DATABASE_URL",
                ..
            }
        ));
    }

    #[test]
    fn bad_cidr_is_rejected_with_the_variable_name() {
        let err = Config::from_lookup(lookup(&[
            DB,
            ("YAPSPACE_TRUSTED_PROXIES", "172.16.0.0/12, nonsense"),
        ]))
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid {
                name: "YAPSPACE_TRUSTED_PROXIES",
                ..
            }
        ));
    }

    #[test]
    fn lists_are_trimmed_and_empty_items_dropped() {
        let c = Config::from_lookup(lookup(&[
            DB,
            ("YAPSPACE_TRUSTED_PROXIES", " 172.16.0.0/12 , 10.0.0.0/8 ,, "),
            ("YAPSPACE_EXTRA_BLOCKED_HOSTS", "a.example, ,b.example"),
        ]))
        .unwrap();
        assert_eq!(c.trusted_proxies.len(), 2);
        assert_eq!(c.extra_blocked_hosts, vec!["a.example", "b.example"]);
    }

    #[test]
    fn bad_numbers_and_bools_are_rejected() {
        let err = Config::from_lookup(lookup(&[DB, ("YAPSPACE_DB_MAX_CONNECTIONS", "many")])).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid {
                name: "YAPSPACE_DB_MAX_CONNECTIONS",
                ..
            }
        ));
        let err = Config::from_lookup(lookup(&[DB, ("YAPSPACE_REGISTRATION_OPEN", "maybe")])).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid {
                name: "YAPSPACE_REGISTRATION_OPEN",
                ..
            }
        ));
        let err = Config::from_lookup(lookup(&[DB, ("YAPSPACE_DB_MAX_CONNECTIONS", "0")])).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid {
                name: "YAPSPACE_DB_MAX_CONNECTIONS",
                ..
            }
        ));
    }

    #[test]
    fn debug_output_redacts_the_database_url() {
        let c = Config::from_lookup(lookup(&[DB])).unwrap();
        let shown = format!("{c:?}");
        assert!(!shown.contains("postgres://"));
        assert!(shown.contains("<redacted>"));
    }
}
