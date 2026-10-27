//! Password hashing with Argon2id (docs/architecture/security.md §H.1). Hashing is CPU-heavy, so it runs on the
//! blocking pool and is limited by a semaphore. Otherwise a burst of logins could exhaust the CPU.

use std::sync::{Arc, OnceLock};

use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher as _, PasswordVerifier as _, Version,
};
use tokio::sync::Semaphore;

/// OWASP minimum for Argon2id: m = 19 MiB, t = 2, p = 1.
const MEMORY_KIB: u32 = 19_456;
const TIME_COST: u32 = 2;
const PARALLELISM: u32 = 1;
const MAX_CONCURRENT_HASHES: usize = 4;

#[derive(Debug)]
pub struct PasswordHasher {
    permits: Arc<Semaphore>,
    dummy_hash: OnceLock<String>,
}

impl PasswordHasher {
    pub fn new() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_HASHES)),
            dummy_hash: OnceLock::new(),
        }
    }

    fn argon2() -> Argon2<'static> {
        let params = Params::new(MEMORY_KIB, TIME_COST, PARALLELISM, None).expect("valid Argon2 parameters");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    }

    /// Hashes `password`. Returns a PHC string that includes the salt and parameters.
    pub async fn hash(&self, password: String) -> anyhow::Result<String> {
        let _permit = self.permits.acquire().await?;
        tokio::task::spawn_blocking(move || {
            // The salt is generated per hash by the password-hash crate.
            Self::argon2()
                .hash_password(password.as_bytes())
                .map(|h| h.to_string())
                .map_err(|e| anyhow::anyhow!("hash: {e}"))
        })
        .await?
    }

    /// Checks `password` against a PHC string. A malformed string is treated as a mismatch.
    pub async fn verify(&self, password: String, phc: String) -> anyhow::Result<bool> {
        let _permit = self.permits.acquire().await?;
        tokio::task::spawn_blocking(move || {
            let parsed = match PasswordHash::new(&phc) {
                Ok(parsed) => parsed,
                Err(_) => return Ok(false),
            };
            Ok(Self::argon2().verify_password(password.as_bytes(), &parsed).is_ok())
        })
        .await?
    }

    /// Runs a verification that always fails, so an unknown username takes about as long as a wrong password.
    pub async fn verify_dummy(&self, password: String) -> anyhow::Result<bool> {
        let phc = match self.dummy_hash.get() {
            Some(phc) => phc.clone(),
            None => {
                let phc = self.hash("dummy-password-for-timing".into()).await?;
                let _ = self.dummy_hash.set(phc.clone());
                phc
            }
        };
        self.verify(password, phc).await
    }

    /// True when a stored hash uses other parameters than the current ones, so it should be rehashed at next login.
    pub fn needs_rehash(phc: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(phc) else {
            return true;
        };
        let want = format!("$argon2id$v=19$m={MEMORY_KIB},t={TIME_COST},p={PARALLELISM}$");
        !phc.starts_with(&want) || parsed.algorithm.as_str() != "argon2id"
    }
}

impl Default for PasswordHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_then_verify() {
        let hasher = PasswordHasher::new();
        let phc = hasher.hash("correct horse battery".into()).await.unwrap();
        assert!(phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(hasher.verify("correct horse battery".into(), phc.clone()).await.unwrap());
        assert!(!hasher.verify("wrong password!!".into(), phc.clone()).await.unwrap());
        assert!(!PasswordHasher::needs_rehash(&phc));
    }

    #[tokio::test]
    async fn malformed_hash_is_a_mismatch_not_an_error() {
        let hasher = PasswordHasher::new();
        assert!(!hasher.verify("anything".into(), "not-a-phc".into()).await.unwrap());
        assert!(PasswordHasher::needs_rehash("not-a-phc"));
    }

    #[tokio::test]
    async fn dummy_verification_always_fails() {
        let hasher = PasswordHasher::new();
        assert!(!hasher.verify_dummy("whatever".into()).await.unwrap());
    }
}
