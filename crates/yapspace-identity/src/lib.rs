//! Pure, versioned URL identity rules for YapSpace (docs/architecture/url-identity.md §E). No IO and no async.
//!
//! The server is the only authority on identity. The client only pre-filters (§E.7). Every rule here is listed in
//! `rules` so that a change to a list is easy to see in review.

pub mod adapters;
mod error;
mod normalize;
pub mod rules;

pub use error::RejectReason;

use sha2::{Digest, Sha256};

/// Ruleset version embedded in every identity key (`v1|…`).
pub const RULESET: u16 = 1;

/// How an identity key was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Generic normalized URL.
    Url,
    /// Canonical URL supplied by the page, accepted under §E.5.
    Canonical,
    /// Site adapter with a stable content id.
    Adapter,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::Canonical => "canonical",
            Self::Adapter => "adapter",
        }
    }
}

/// One identity the server can look up or create under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The identity key, for example `v1|url|example.com/a?id=3`.
    pub key: String,
    pub kind: Kind,
    /// Human-readable URL shown in the panel.
    pub display_url: String,
    /// Host without port, used as the discussion's `site`.
    pub site: String,
}

/// SHA-256 of the key. This is the primary key in `discussion_identities`.
pub fn key_hash(key: &str) -> [u8; 32] {
    Sha256::digest(key.as_bytes()).into()
}

/// Computes the identity candidates for a page.
///
/// The first candidate is the one to create a discussion under. Lookups try the candidates in order, and the first hit
/// wins (§E.5).
///
/// * `observed`: the URL the browser reports.
/// * `canonical`: the page's canonical URL, if it has one.
/// * `extra_blocked_hosts`: hosts from `YAPSPACE_EXTRA_BLOCKED_HOSTS`.
pub fn candidates(
    observed: &str,
    canonical: Option<&str>,
    extra_blocked_hosts: &[String],
) -> Result<Vec<Candidate>, RejectReason> {
    let obs = normalize::normalize(observed, extra_blocked_hosts)?;

    // A site adapter matched the observed URL, so its stable content id is the only identity.
    if let Some(adapter) = adapters::adapter_key_for(observed) {
        return Ok(vec![Candidate {
            key: format!("v{RULESET}|{}|{}", adapter.adapter, adapter.value),
            kind: Kind::Adapter,
            display_url: adapter.display_url,
            site: obs.site.clone(),
        }]);
    }

    let obs_candidate = Candidate {
        key: format!("v{RULESET}|url|{}", obs.value),
        kind: Kind::Url,
        display_url: obs.display_url.clone(),
        site: obs.site.clone(),
    };

    let Some(canonical) = canonical else {
        return Ok(vec![obs_candidate]);
    };
    let Some(accepted) = accept_canonical(&obs, canonical, extra_blocked_hosts) else {
        return Ok(vec![obs_candidate]);
    };
    Ok(vec![accepted, obs_candidate])
}

/// Applies the canonical acceptance rules from §E.5 step 3. Returns `None` for any canonical that must be ignored.
fn accept_canonical(
    obs: &normalize::Normalized,
    canonical: &str,
    extra_blocked_hosts: &[String],
) -> Option<Candidate> {
    // The same-site rule applies to every canonical, adapter matches included. Otherwise a page could attach itself
    // to another site's discussion.
    let can_url = url::Url::parse(canonical).ok()?;
    let can_host = normalize::plain_host(&can_url)?;
    if registrable(&can_host) != registrable(&obs.host) {
        return None;
    }

    // An adapter match on the canonical names the same content, so its stable id is used.
    if let Some(adapter) = adapters::adapter_key_for(canonical) {
        return Some(Candidate {
            key: format!("v{RULESET}|{}|{}", adapter.adapter, adapter.value),
            kind: Kind::Adapter,
            display_url: adapter.display_url,
            site: obs.site.clone(),
        });
    }

    let can = normalize::normalize(canonical, extra_blocked_hosts).ok()?;
    // A deep page whose canonical points at the homepage is a known CMS bug, so ignore it.
    if obs.path != "/" && can.path == "/" {
        return None;
    }
    if can.value == obs.value {
        return None;
    }
    Some(Candidate {
        key: format!("v{RULESET}|canonical|{}", can.value),
        kind: Kind::Canonical,
        display_url: can.display_url,
        site: obs.site.clone(),
    })
}

/// eTLD+1 of a host, used to decide whether a canonical is on the same site.
fn registrable(host: &str) -> Option<String> {
    psl::domain_str(host).map(str::to_owned)
}

/// Checks a single URL and returns its generic normalized form. Used by `POST /v1/discussions/resolve` for
/// validation, and by tests.
pub fn normalize_url(input: &str, extra_blocked_hosts: &[String]) -> Result<String, RejectReason> {
    normalize::normalize(input, extra_blocked_hosts).map(|n| n.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_is_v1() {
        assert_eq!(RULESET, 1);
    }

    #[test]
    fn key_hash_is_stable_and_32_bytes() {
        assert_eq!(key_hash("v1|url|a.com/").len(), 32);
        assert_eq!(key_hash("x"), key_hash("x"));
        assert_ne!(key_hash("x"), key_hash("y"));
    }
}
