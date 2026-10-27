//! In-process rate limits (docs/architecture/security.md §H.4). Each bucket is a keyed GCRA limiter. The limits are per
//! process, which is acceptable for one instance and is documented in P6-02 for several.

use std::{hash::Hash, num::NonZeroU32, time::Duration};

use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
    state::keyed::DefaultKeyedStateStore,
};
use std::net::IpAddr;
use uuid::Uuid;

use crate::error::AppError;

type Keyed<K> = RateLimiter<K, DefaultKeyedStateStore<K>, DefaultClock>;

/// One named limit, keyed by `K`.
pub struct Bucket<K: Hash + Eq + Clone> {
    name: &'static str,
    enabled: bool,
    clock: DefaultClock,
    limiter: Keyed<K>,
}

impl<K: Hash + Eq + Clone> Bucket<K> {
    pub fn new(name: &'static str, quota: Quota, enabled: bool) -> Self {
        Self {
            name,
            enabled,
            clock: DefaultClock::default(),
            limiter: RateLimiter::keyed(quota),
        }
    }

    /// Consumes one unit for `key`. A refusal names how long the caller must wait.
    pub fn check(&self, key: &K) -> Result<(), AppError> {
        if !self.enabled {
            return Ok(());
        }
        match self.limiter.check_key(key) {
            Ok(()) => Ok(()),
            Err(not_until) => {
                metrics::counter!("ratelimit_rejections_total", "bucket" => self.name).increment(1);
                let wait = not_until.wait_time_from(self.clock.now());
                // Round up, and always tell the client to wait at least one second.
                let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
                Err(AppError::RateLimited {
                    retry_after_s: seconds.max(1),
                })
            }
        }
    }

    /// Drops state for keys that are no longer limited. Run from a background task.
    pub fn retain_recent(&self) {
        self.limiter.retain_recent();
    }
}

fn nz(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("quota values are non-zero")
}

/// Every limit in the API, with the defaults from the plan (§H.4).
pub struct Limits {
    pub register: Bucket<IpAddr>,
    pub login_ip: Bucket<IpAddr>,
    pub login_ip_user: Bucket<String>,
    pub recover: Bucket<IpAddr>,
    pub resolve: Bucket<IpAddr>,
    pub create_discussion: Bucket<Uuid>,
    /// Established accounts: a burst of 5, then one message every 3 seconds.
    pub post_user: Bucket<Uuid>,
    /// Accounts younger than 24 hours: a burst of 3, then one message every 30 seconds.
    pub post_user_new: Bucket<Uuid>,
    pub post_ip: Bucket<IpAddr>,
    /// Backstop for every request from one client.
    pub global_ip: Bucket<IpAddr>,
}

impl Limits {
    pub fn new(enabled: bool) -> Self {
        let period = |secs: u64| Duration::from_secs(secs);
        Self {
            register: Bucket::new("register", Quota::per_hour(nz(5)), enabled),
            login_ip: Bucket::new(
                "login_ip",
                Quota::with_period(period(15 * 60 / 30))
                    .expect("valid quota")
                    .allow_burst(nz(30)),
                enabled,
            ),
            login_ip_user: Bucket::new(
                "login_ip_user",
                Quota::with_period(period(15 * 60 / 10))
                    .expect("valid quota")
                    .allow_burst(nz(10)),
                enabled,
            ),
            recover: Bucket::new("recover", Quota::per_hour(nz(5)), enabled),
            resolve: Bucket::new("resolve", Quota::per_minute(nz(120)), enabled),
            create_discussion: Bucket::new("create_discussion", Quota::per_hour(nz(30)), enabled),
            post_user: Bucket::new(
                "post_user",
                Quota::with_period(period(3))
                    .expect("valid quota")
                    .allow_burst(nz(5)),
                enabled,
            ),
            post_user_new: Bucket::new(
                "post_user_new",
                Quota::with_period(period(30))
                    .expect("valid quota")
                    .allow_burst(nz(3)),
                enabled,
            ),
            post_ip: Bucket::new("post_ip", Quota::per_minute(nz(60)), enabled),
            global_ip: Bucket::new("global_ip", Quota::per_minute(nz(600)), enabled),
        }
    }

    /// Drops idle state from every bucket. Called once a minute.
    pub fn retain_recent(&self) {
        self.register.retain_recent();
        self.login_ip.retain_recent();
        self.login_ip_user.retain_recent();
        self.recover.retain_recent();
        self.resolve.retain_recent();
        self.create_discussion.retain_recent();
        self.post_user.retain_recent();
        self.post_user_new.retain_recent();
        self.post_ip.retain_recent();
        self.global_ip.retain_recent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_request_in_a_burst_of_two_is_refused_with_a_wait() {
        let bucket: Bucket<u8> = Bucket::new("test", Quota::per_hour(nz(2)), true);
        assert!(bucket.check(&1).is_ok());
        assert!(bucket.check(&1).is_ok());
        match bucket.check(&1) {
            Err(AppError::RateLimited { retry_after_s }) => assert!(retry_after_s >= 1),
            other => panic!("expected rate limit, got {other:?}"),
        }
        // Other keys have their own budget.
        assert!(bucket.check(&2).is_ok());
    }

    #[test]
    fn disabled_buckets_never_refuse() {
        let bucket: Bucket<u8> = Bucket::new("test", Quota::per_hour(nz(1)), false);
        for _ in 0..10 {
            assert!(bucket.check(&1).is_ok());
        }
    }
}
