//! Background jobs (docs/architecture/security.md §H.5, deployment.md retention). Each job runs on its own schedule and
//! stops when shutdown starts. Every run is counted in `jobs_runs_total{job,result}`.

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};

use crate::{
    app::AppState,
    db::{account, reports},
};

/// Rows changed per statement in the erasure jobs, so no single statement holds locks for long.
const BATCH: i64 = 1000;
/// Batches per run. Anything left over is picked up on the next tick.
const MAX_BATCHES_PER_RUN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// Expired and revoked sessions, 30 days after they end.
    PurgeSessions,
    /// Bodies of messages from deleted accounts. Runs every minute.
    EraseDeletedAccountContent,
    /// Bodies of removed messages, 90 days after removal.
    EraseRemovedBodies,
    /// Resolved reports, after one year.
    PurgeResolvedReports,
    /// Audit events, after two years.
    PurgeAudit,
}

impl Job {
    pub fn name(self) -> &'static str {
        match self {
            Self::PurgeSessions => "purge_sessions",
            Self::EraseDeletedAccountContent => "erase_deleted_account_content",
            Self::EraseRemovedBodies => "erase_removed_bodies",
            Self::PurgeResolvedReports => "purge_resolved_reports",
            Self::PurgeAudit => "purge_audit",
        }
    }

    pub fn period(self) -> Duration {
        match self {
            Self::PurgeSessions => Duration::from_secs(60 * 60),
            Self::EraseDeletedAccountContent => Duration::from_secs(60),
            Self::EraseRemovedBodies | Self::PurgeResolvedReports | Self::PurgeAudit => {
                Duration::from_secs(24 * 60 * 60)
            }
        }
    }
}

/// Runs one job once and returns how many rows it changed. Tests call this directly.
pub async fn run_once(state: &AppState, job: Job) -> anyhow::Result<u64> {
    let now = Utc::now();
    let changed = match job {
        Job::PurgeSessions => {
            account::purge_sessions_before(&state.db, now - ChronoDuration::days(30)).await?
        }
        Job::EraseDeletedAccountContent => {
            let mut total = 0;
            for _ in 0..MAX_BATCHES_PER_RUN {
                let changed = account::erase_deleted_users_content(&state.db, BATCH).await?;
                total += changed;
                if (changed as i64) < BATCH {
                    break;
                }
            }
            total
        }
        Job::EraseRemovedBodies => {
            let cutoff = now - ChronoDuration::days(90);
            let mut total = 0;
            for _ in 0..MAX_BATCHES_PER_RUN {
                let changed = account::erase_removed_before(&state.db, cutoff, BATCH).await?;
                total += changed;
                if (changed as i64) < BATCH {
                    break;
                }
            }
            total
        }
        Job::PurgeResolvedReports => {
            reports::purge_resolved_before(&state.db, now - ChronoDuration::days(365)).await?
        }
        Job::PurgeAudit => account::purge_audit_before(&state.db, now - ChronoDuration::days(730)).await?,
    };
    Ok(changed)
}

/// Starts every job loop. Each loop waits one period before its first run, so startup stays quiet.
pub fn spawn_all(state: &AppState) {
    for job in [
        Job::PurgeSessions,
        Job::EraseDeletedAccountContent,
        Job::EraseRemovedBodies,
        Job::PurgeResolvedReports,
        Job::PurgeAudit,
    ] {
        state.tasks.spawn(run_loop(state.clone(), job));
    }
    state.tasks.spawn(ratelimit_cleanup(state.clone()));
}

async fn run_loop(state: AppState, job: Job) {
    let mut ticker = tokio::time::interval(job.period());
    ticker.tick().await;
    loop {
        tokio::select! {
            () = state.shutdown.cancelled() => break,
            _ = ticker.tick() => {
                match run_once(&state, job).await {
                    Ok(changed) => {
                        metrics::counter!("jobs_runs_total", "job" => job.name(), "result" => "ok").increment(1);
                        if changed > 0 {
                            tracing::info!(job = job.name(), changed, "job finished");
                        }
                    }
                    Err(err) => {
                        metrics::counter!("jobs_runs_total", "job" => job.name(), "result" => "error").increment(1);
                        tracing::error!(job = job.name(), error = %err, "job failed");
                    }
                }
            }
        }
    }
}

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
