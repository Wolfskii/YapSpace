//! Site adapters (docs/architecture/url-identity.md §E.4). An adapter turns a permalink into a stable content id, so
//! every URL of the same post maps to one discussion. An adapter that matches a host but not a pattern returns `None`,
//! and the generic rules apply.
//!
//! Adding an adapter changes the keys for its site, so it needs a ruleset bump or must only apply to URLs with no
//! existing discussion (§E.4).

use url::Url;

use crate::normalize::plain_host;

/// A stable content identity produced by an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterKey {
    /// Adapter name used in the key, for example `youtube`.
    pub adapter: &'static str,
    /// Adapter-specific value, for example `video:dQw4w9WgXcQ`.
    pub value: String,
    /// Canonical display URL for the content.
    pub display_url: String,
}

/// One site adapter.
pub trait SiteAdapter: Sync {
    fn id(&self) -> &'static str;
    fn matches_host(&self, host: &str) -> bool;
    fn key(&self, url: &Url) -> Option<AdapterKey>;
}

const ADAPTERS: &[&dyn SiteAdapter] = &[&YouTube, &X, &Reddit, &HackerNews];

/// The adapter key for `input`, if an adapter recognises the host and the pattern. Returns `None` for unparseable
/// input, for other hosts, and for paths that aren't permalinks.
pub fn adapter_key_for(input: &str) -> Option<AdapterKey> {
    let url = Url::parse(input).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = plain_host(&url)?;
    ADAPTERS
        .iter()
        .filter(|adapter| adapter.matches_host(&host))
        .find_map(|adapter| adapter.key(&url))
}

/// Path segments with empty parts removed.
fn segments(url: &Url) -> Vec<&str> {
    url.path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default()
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// YouTube video ids are 11 characters of `[A-Za-z0-9_-]`.
fn youtube_id(s: &str) -> Option<&str> {
    (s.len() == 11
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then_some(s)
}

struct YouTube;

impl SiteAdapter for YouTube {
    fn id(&self) -> &'static str {
        "youtube"
    }

    fn matches_host(&self, host: &str) -> bool {
        matches!(
            host,
            "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" | "youtube-nocookie.com"
        )
    }

    fn key(&self, url: &Url) -> Option<AdapterKey> {
        let parts = segments(url);
        let id = if plain_host(url)? == "youtu.be" {
            youtube_id(parts.first()?)?.to_owned()
        } else {
            match parts.as_slice() {
                ["watch"] => url
                    .query_pairs()
                    .find(|(k, _)| k == "v")
                    .and_then(|(_, v)| youtube_id(&v).map(str::to_owned))?,
                ["shorts" | "live" | "embed", id] => youtube_id(id)?.to_owned(),
                _ => return None,
            }
        };
        Some(AdapterKey {
            adapter: self.id(),
            display_url: format!("https://www.youtube.com/watch?v={id}"),
            value: format!("video:{id}"),
        })
    }
}

struct X;

impl SiteAdapter for X {
    fn id(&self) -> &'static str {
        "x"
    }

    fn matches_host(&self, host: &str) -> bool {
        matches!(
            host,
            "x.com" | "twitter.com" | "mobile.x.com" | "mobile.twitter.com"
        )
    }

    fn key(&self, url: &Url) -> Option<AdapterKey> {
        let parts = segments(url);
        let id = match parts.as_slice() {
            // /{user}/status/{id}, with optional /photo/N, /video/N or /analytics after it
            [_, "status", id, ..] if all_digits(id) => *id,
            ["i", "web", "status", id] if all_digits(id) => *id,
            _ => return None,
        };
        Some(AdapterKey {
            adapter: self.id(),
            display_url: format!("https://x.com/i/status/{id}"),
            value: format!("post:{id}"),
        })
    }
}

struct Reddit;

impl SiteAdapter for Reddit {
    fn id(&self) -> &'static str {
        "reddit"
    }

    fn matches_host(&self, host: &str) -> bool {
        matches!(
            host,
            "reddit.com" | "old.reddit.com" | "new.reddit.com" | "np.reddit.com" | "m.reddit.com" | "redd.it"
        )
    }

    fn key(&self, url: &Url) -> Option<AdapterKey> {
        let parts = segments(url);
        let raw = if plain_host(url)? == "redd.it" {
            parts.first()?.to_string()
        } else {
            match parts.as_slice() {
                ["r", _, "comments", id, ..] => id.to_string(),
                ["comments", id, ..] => id.to_string(),
                _ => return None,
            }
        };
        let id = raw.to_ascii_lowercase();
        if !(3..=10).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return None;
        }
        Some(AdapterKey {
            adapter: self.id(),
            display_url: format!("https://www.reddit.com/comments/{id}"),
            value: format!("post:{id}"),
        })
    }
}

struct HackerNews;

impl SiteAdapter for HackerNews {
    fn id(&self) -> &'static str {
        "hn"
    }

    fn matches_host(&self, host: &str) -> bool {
        host == "news.ycombinator.com"
    }

    fn key(&self, url: &Url) -> Option<AdapterKey> {
        if url.path() != "/item" {
            return None;
        }
        let id = url
            .query_pairs()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.into_owned())?;
        if !all_digits(&id) {
            return None;
        }
        Some(AdapterKey {
            adapter: self.id(),
            display_url: format!("https://news.ycombinator.com/item?id={id}"),
            value: format!("item:{id}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_forms_resolve_to_the_same_video() {
        for input in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10",
            "https://youtu.be/dQw4w9WgXcQ?si=abc",
            "https://m.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
        ] {
            assert_eq!(
                adapter_key_for(input).unwrap().value,
                "video:dQw4w9WgXcQ",
                "{input}"
            );
        }
    }

    #[test]
    fn youtube_channel_page_falls_through() {
        assert!(adapter_key_for("https://www.youtube.com/@somechannel").is_none());
    }

    #[test]
    fn reddit_share_link_falls_through_to_generic() {
        assert!(adapter_key_for("https://www.reddit.com/r/rust/s/AbCdEfGh").is_none());
    }

    #[test]
    fn non_permalink_on_known_host_falls_through() {
        assert!(adapter_key_for("https://news.ycombinator.com/newest").is_none());
        assert!(adapter_key_for("https://x.com/jack").is_none());
    }
}
