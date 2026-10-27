//! Background jobs. Each one stops when shutdown starts.

use std::time::Duration;

use crate::app::AppState;

const RATELIMIT_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Drops idle rate-limit state once a minute, so memory stays bounded by the number of recently active clients.
pub async fn ratelimit_cleanup(state: AppState) {
    let mut ticker = tokio::time::interval(RATELIMIT_CLEANUP_INTERVAL);
    loop {
        tokio::select! {
            () = state.shutdown.cancelled() => break,
            _ = ticker.tick() => state.limits.retain_recent(),
        }
    }
}
