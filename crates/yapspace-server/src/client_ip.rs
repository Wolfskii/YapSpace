//! Client IP for rate limits and logs (docs/architecture/security.md §H.2, §H.4).
//!
//! `X-Forwarded-For` is only read when the TCP peer is a trusted proxy (`YAPSPACE_TRUSTED_PROXIES`). Otherwise a
//! client could pick its own address. IPv6 is keyed by its /64 prefix, because one host usually owns a whole /64.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use axum::{
    extract::{ConnectInfo, FromRef, FromRequestParts},
    http::{HeaderMap, request::Parts},
};
use ipnet::IpNet;

use crate::{app::AppState, error::AppError};

const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// Returns the address to limit by. The peer is used unless it is a trusted proxy, in which case the right-most
/// `X-Forwarded-For` entry that parses is used. The right-most entry is the one the trusted proxy added.
pub fn resolve_ip(peer: IpAddr, headers: &HeaderMap, trusted_proxies: &[IpNet]) -> IpAddr {
    let from_trusted = trusted_proxies.iter().any(|net| net.contains(&peer));
    let forwarded = from_trusted
        .then(|| headers.get(X_FORWARDED_FOR))
        .flatten()
        .and_then(|value| value.to_str().ok())
        .and_then(|list| {
            list.rsplit(',')
                .find_map(|part| part.trim().parse::<IpAddr>().ok())
        });
    normalize(forwarded.unwrap_or(peer))
}

/// Masks IPv6 to its /64 prefix. IPv4 is returned unchanged.
pub fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            let bits = u128::from(v6) & (u128::MAX << 64);
            IpAddr::V6(Ipv6Addr::from(bits))
        }
    }
}

/// The client's address, as resolved by [`resolve_ip`]. Requests without connection info (in-process tests) count as
/// loopback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientIp(pub IpAddr);

impl<S> FromRequestParts<S> for ClientIp
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        Ok(ClientIp(resolve_ip(
            peer,
            &parts.headers,
            &state.config.trusted_proxies,
        )))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(X_FORWARDED_FOR, HeaderValue::from_str(value).unwrap());
        map
    }

    fn proxy_net() -> Vec<IpNet> {
        vec!["10.0.0.0/8".parse().unwrap()]
    }

    #[test]
    fn forwarded_header_from_an_untrusted_peer_is_ignored() {
        let peer: IpAddr = "203.0.113.9".parse().unwrap();
        assert_eq!(resolve_ip(peer, &headers("198.51.100.1"), &proxy_net()), peer);
    }

    #[test]
    fn right_most_forwarded_entry_is_used_from_a_trusted_proxy() {
        let peer: IpAddr = "10.1.2.3".parse().unwrap();
        let resolved = resolve_ip(peer, &headers("1.1.1.1, 198.51.100.7"), &proxy_net());
        assert_eq!(resolved, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn garbage_forwarded_entries_fall_back_to_the_peer() {
        let peer: IpAddr = "10.1.2.3".parse().unwrap();
        assert_eq!(resolve_ip(peer, &headers("not-an-ip"), &proxy_net()), peer);
    }

    #[test]
    fn ipv6_is_keyed_by_its_slash_64() {
        let a: IpAddr = "2001:db8:1:2:aaaa:bbbb:cccc:dddd".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:1111:2222:3333:4444".parse().unwrap();
        assert_eq!(normalize(a), normalize(b));
        assert_eq!(normalize(a), "2001:db8:1:2::".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn ipv4_is_unchanged() {
        let ip: IpAddr = "192.0.2.5".parse().unwrap();
        assert_eq!(normalize(ip), ip);
    }
}
