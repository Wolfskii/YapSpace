//! Database access: connection pool, migrations, and pool metrics. SQL lives in the sub-modules (added in Phase 2).

use std::{str::FromStr, time::Duration};

use sqlx::{
    PgPool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tokio_util::sync::CancellationToken;

pub mod audit;
pub mod credentials;
pub mod discussions;
pub mod sessions;
pub mod users;

use crate::config::Config;

/// Migrations are embedded in the binary. Files in `migrations/` are immutable once committed (AGENTS.md).
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(3);
const STATEMENT_TIMEOUT_MS: &str = "5000";
const POOL_METRICS_INTERVAL: Duration = Duration::from_secs(5);

/// Creates the pool. Connections open on first use, so this does not need a running database.
pub fn pool(config: &Config) -> anyhow::Result<PgPool> {
    let options = PgConnectOptions::from_str(&config.database_url)?
        .application_name("yapspace")
        .options([("statement_timeout", STATEMENT_TIMEOUT_MS)]);
    Ok(PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_lazy_with(options))
}

/// Applies pending migrations. Running it again when nothing is pending does nothing.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

/// Publishes pool gauges every few seconds until `shutdown` is cancelled.
pub async fn pool_metrics_loop(pool: PgPool, shutdown: CancellationToken) {
    let mut ticker = tokio::time::interval(POOL_METRICS_INTERVAL);
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = ticker.tick() => {
                let size = pool.size() as usize;
                let idle = pool.num_idle();
                metrics::gauge!("db_pool_connections", "state" => "idle").set(idle as f64);
                metrics::gauge!("db_pool_connections", "state" => "active")
                    .set(size.saturating_sub(idle) as f64);
            }
        }
    }
}
