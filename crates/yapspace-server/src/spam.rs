//! Duplicate-message and new-account spam checks, kept in memory with expiry (docs/architecture/security.md §H.4).

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use moka::sync::Cache;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Identical messages from one user are refused for this long.
const DUPLICATE_TTL: Duration = Duration::from_secs(10 * 60);
/// Accounts younger than a day may post in at most `NEW_ACCOUNT_DISCUSSION_LIMIT` discussions in this window.
const NEW_ACCOUNT_WINDOW: Duration = Duration::from_secs(5 * 60);
pub const NEW_ACCOUNT_DISCUSSION_LIMIT: usize = 10;

pub struct SpamGuard {
    /// `(user, fingerprint of the body)` for recently posted messages.
    bodies: Cache<(Uuid, [u8; 32]), ()>,
    /// Discussions a user posted into in the last few minutes.
    discussions: Cache<Uuid, Arc<Mutex<HashSet<Uuid>>>>,
}

impl SpamGuard {
    pub fn new() -> Self {
        Self {
            bodies: Cache::builder()
                .time_to_live(DUPLICATE_TTL)
                .max_capacity(200_000)
                .build(),
            discussions: Cache::builder()
                .time_to_live(NEW_ACCOUNT_WINDOW)
                .max_capacity(100_000)
                .build(),
        }
    }

    pub fn is_duplicate(&self, user: Uuid, body: &str) -> bool {
        self.bodies.contains_key(&(user, fingerprint(body)))
    }

    pub fn remember(&self, user: Uuid, body: &str) {
        self.bodies.insert((user, fingerprint(body)), ());
    }

    /// Records that `user` posted into `discussion`. Returns how many distinct discussions the user has posted into
    /// recently, including this one.
    pub fn note_discussion(&self, user: Uuid, discussion: Uuid) -> usize {
        let set = self
            .discussions
            .get_with(user, || Arc::new(Mutex::new(HashSet::new())));
        let mut guard = set.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.insert(discussion);
        guard.len()
    }
}

impl Default for SpamGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Fingerprint of a body. Case and whitespace are ignored, so trivial edits do not get past the check.
pub fn fingerprint(body: &str) -> [u8; 32] {
    let normalized = body
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ");
    Sha256::digest(normalized.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_ignores_case_and_whitespace() {
        assert_eq!(fingerprint("Hello   World"), fingerprint("hello world\n"));
        assert_ne!(fingerprint("hello world"), fingerprint("hello there"));
    }

    #[test]
    fn duplicates_are_remembered_per_user() {
        let guard = SpamGuard::new();
        let alice = Uuid::now_v7();
        let bob = Uuid::now_v7();
        guard.remember(alice, "buy now");
        assert!(guard.is_duplicate(alice, "BUY   now"));
        assert!(!guard.is_duplicate(bob, "buy now"));
    }

    #[test]
    fn distinct_discussions_are_counted() {
        let guard = SpamGuard::new();
        let user = Uuid::now_v7();
        assert_eq!(guard.note_discussion(user, Uuid::now_v7()), 1);
        assert_eq!(guard.note_discussion(user, Uuid::now_v7()), 2);
        let same = Uuid::now_v7();
        guard.note_discussion(user, same);
        assert_eq!(guard.note_discussion(user, same), 3);
    }
}
