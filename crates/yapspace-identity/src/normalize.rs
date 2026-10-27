//! Generic URL normalization, steps 1–9 of docs/architecture/url-identity.md §E.3. The order matters and must not change.

use url::{Host, Url};

use crate::{
    RejectReason,
    rules::{
        BLOCKED_HOSTS, PRIVATE_HOST_SUFFIXES, SECRET_LIKE_MIN_LEN, SENSITIVE_PARAMS, SENSITIVE_PREFIXES,
        TRACKING_PARAMS, TRACKING_PREFIXES,
    },
};

/// Longest accepted input, in bytes (§F.1, `url_unsupported/too_long`).
pub const MAX_INPUT_LEN: usize = 2048;

/// Result of the generic rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    /// Host with a port if the port is not the default one.
    pub host: String,
    /// Normalized path (no query, no fragment).
    pub path: String,
    /// The identity value without the `v1|kind|` prefix: `host[:port]/path[?query][#route]`.
    pub value: String,
    /// `https://` + value, or `http://` if the observed URL was http.
    pub display_url: String,
    /// Host without port.
    pub site: String,
}

pub fn normalize(input: &str, extra_blocked_hosts: &[String]) -> Result<Normalized, RejectReason> {
    // 1. Parse, and enforce the length limit.
    if input.len() > MAX_INPUT_LEN {
        return Err(RejectReason::TooLong);
    }
    let url = Url::parse(input).map_err(|_| RejectReason::Invalid)?;

    // 2. Scheme.
    let scheme = match url.scheme() {
        "http" => "http",
        "https" => "https",
        _ => return Err(RejectReason::UnsupportedScheme),
    };

    // 3. Userinfo is dropped by construction below (it is never read).

    // 4. Host.
    let host = match url.host().ok_or(RejectReason::Invalid)? {
        Host::Ipv4(_) | Host::Ipv6(_) => return Err(RejectReason::IpHost),
        Host::Domain(domain) => domain.to_ascii_lowercase(),
    };
    let host = host.strip_suffix('.').unwrap_or(&host).to_owned();
    if host.is_empty() || !host.contains('.') || is_private_name(&host) {
        return Err(RejectReason::PrivateHost);
    }
    let host = strip_one_www(&host);
    if is_blocked(&host, extra_blocked_hosts) {
        return Err(RejectReason::BlockedHost);
    }

    // 5. Port (the url crate already drops the default port).
    let site = host.clone();
    let host_with_port = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };

    // 6. Path.
    let raw_path = url.path();
    if raw_path.contains(';') {
        return Err(RejectReason::SensitiveParameters);
    }
    let path = normalize_path(raw_path)?;

    // 7. Query.
    let query = normalize_query(url.query().unwrap_or(""))?;

    // 8. Fragment. Hash routes such as `#/board/42` are kept. Everything else (tokens, anchors) is dropped.
    let route = match url.fragment() {
        Some(fragment) if fragment.starts_with('/') || fragment.starts_with("!/") => {
            let route_path = fragment.split('?').next().unwrap_or("");
            Some(normalize_path(route_path)?)
        }
        _ => None,
    };

    // 9. Assemble.
    let mut value = format!("{host_with_port}{path}");
    if !query.is_empty() {
        value.push('?');
        value.push_str(&query);
    }
    if let Some(route) = &route {
        value.push('#');
        value.push_str(route);
    }
    let display_url = format!("{scheme}://{value}");

    Ok(Normalized {
        host: site.clone(),
        path,
        value,
        display_url,
        site,
    })
}

/// The host with the same rules as step 4 (lowercase, one trailing dot removed, one `www.` removed). Adapters match
/// on this, so they agree with the generic rules about which host a URL is on.
pub fn plain_host(url: &Url) -> Option<String> {
    let host = match url.host()? {
        Host::Domain(domain) => domain.to_ascii_lowercase(),
        _ => return None,
    };
    let host = host.strip_suffix('.').unwrap_or(&host).to_owned();
    Some(strip_one_www(&host))
}

fn is_private_name(host: &str) -> bool {
    host == "localhost" || PRIVATE_HOST_SUFFIXES.iter().any(|suffix| host.ends_with(suffix))
}

fn is_blocked(host: &str, extra: &[String]) -> bool {
    BLOCKED_HOSTS
        .iter()
        .copied()
        .chain(extra.iter().map(String::as_str))
        .any(|blocked| host == blocked || host.ends_with(&format!(".{blocked}")))
}

fn strip_one_www(host: &str) -> String {
    match host.strip_prefix("www.") {
        Some(rest) if rest.contains('.') => rest.to_owned(),
        _ => host.to_owned(),
    }
}

/// Step 6b–6f: percent-encoding, repeated slashes, trailing slash, and secret-looking segments.
fn normalize_path(raw: &str) -> Result<String, RejectReason> {
    let encoded = normalize_percent(raw);
    let mut collapsed = String::with_capacity(encoded.len());
    let mut previous_slash = false;
    for c in encoded.chars() {
        if c == '/' {
            if previous_slash {
                continue;
            }
            previous_slash = true;
        } else {
            previous_slash = false;
        }
        collapsed.push(c);
    }
    if collapsed.len() > 1 && collapsed.ends_with('/') {
        collapsed.pop();
    }
    if collapsed.split('/').any(is_secret_like) {
        return Err(RejectReason::SecretLikeToken);
    }
    Ok(collapsed)
}

/// Step 7: sorts parameters, removes tracking parameters, and refuses sensitive ones.
fn normalize_query(raw: &str) -> Result<String, RejectReason> {
    let mut pairs: Vec<(String, Option<String>)> = Vec::new();
    for part in raw.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = match part.split_once('=') {
            Some((k, v)) => (normalize_percent(k), Some(normalize_percent(v))),
            None => (normalize_percent(part), None),
        };
        let lower = normalize_percent(&key).to_ascii_lowercase();

        if SENSITIVE_PARAMS.contains(&lower.as_str())
            || SENSITIVE_PREFIXES.iter().any(|p| lower.starts_with(p))
        {
            return Err(RejectReason::SensitiveParameters);
        }
        if value.as_deref().is_some_and(is_secret_like) {
            return Err(RejectReason::SecretLikeToken);
        }
        if TRACKING_PARAMS.contains(&lower.as_str()) || TRACKING_PREFIXES.iter().any(|p| lower.starts_with(p))
        {
            continue;
        }
        pairs.push((key, value));
    }
    pairs.sort();
    Ok(pairs
        .into_iter()
        .map(|(k, v)| match v {
            Some(v) => format!("{k}={v}"),
            None => k,
        })
        .collect::<Vec<_>>()
        .join("&"))
}

/// Decodes unreserved characters and uppercases the hex of everything else (RFC 3986 §2.1 normalization).
pub fn normalize_percent(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
        {
            let byte = hi * 16 + lo;
            if is_unreserved(byte) {
                out.push(byte);
            } else {
                out.extend_from_slice(format!("%{:02X}", byte).as_bytes());
            }
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    // Only ASCII bytes are inserted, so the output stays valid UTF-8 when the input was.
    String::from_utf8(out).unwrap_or_else(|_| input.to_owned())
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

/// A value of at least 32 characters, made of `[A-Za-z0-9_-]`, with an upper-case letter, a lower-case letter, and a
/// digit. Lowercase hex commit SHAs and slugs do not match.
pub fn is_secret_like(value: &str) -> bool {
    if value.len() < SECRET_LIKE_MIN_LEN {
        return false;
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return false;
    }
    value.bytes().any(|b| b.is_ascii_uppercase())
        && value.bytes().any(|b| b.is_ascii_lowercase())
        && value.bytes().any(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_normalization_decodes_unreserved_and_uppercases_rest() {
        assert_eq!(normalize_percent("%7Eabc%2f%2fx"), "~abc%2F%2Fx");
        assert_eq!(normalize_percent("%41%41"), "AA");
        assert_eq!(normalize_percent("100%"), "100%");
        assert_eq!(normalize_percent("%zz"), "%zz");
    }

    #[test]
    fn secret_detector_needs_mixed_case_and_digit() {
        assert!(is_secret_like("aB3dEfGhIjKlMnOpQrStUvWxYz012345"));
        assert!(!is_secret_like("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_secret_like("aB3dEf"));
    }

    #[test]
    fn trailing_slash_and_double_slashes_are_collapsed_but_root_kept() {
        assert_eq!(normalize_path("/").unwrap(), "/");
        assert_eq!(normalize_path("//a//b/").unwrap(), "/a/b");
    }
}
