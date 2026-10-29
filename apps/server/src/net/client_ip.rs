//! The client IP (008 RF-A1, ADR-0008 D7).

use std::net::IpAddr;

use axum::http::{HeaderMap, HeaderName};

use super::ip_key::normalize;
use super::trusted::TrustedProxies;

pub const X_CLIENT_IP: HeaderName = HeaderName::from_static("x-client-ip");
pub const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// The client IP, always normalized (IPv4-mapped IPv6 becomes IPv4):
/// - internal request (valid `X-Internal-Secret`): exactly one `X-Client-Ip` that parses as
///   an IP as sent (no trimming); otherwise the peer;
/// - peer in `TRUSTED_PROXIES`: the rightmost `X-Forwarded-For` entry that is not a proxy,
///   reading every line in order; a malformed entry ends the search and gives the peer;
/// - any other peer: the peer, without reading `X-Forwarded-For`.
pub fn client_ip(
    peer: IpAddr,
    headers: &HeaderMap,
    internal: bool,
    trusted: &TrustedProxies,
) -> IpAddr {
    let peer = normalize(peer);
    if internal {
        return internal_ip(headers).unwrap_or(peer);
    }
    if !trusted.contains(peer) {
        return peer;
    }
    forwarded_ip(headers, trusted).unwrap_or(peer)
}

fn internal_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let mut values = headers.get_all(X_CLIENT_IP).iter();
    match (values.next(), values.next()) {
        (Some(value), None) => value.to_str().ok()?.parse::<IpAddr>().ok().map(normalize),
        _ => None,
    }
}

/// `None` means "use the peer": no untrusted entry, or a malformed one before it.
fn forwarded_ip(headers: &HeaderMap, trusted: &TrustedProxies) -> Option<IpAddr> {
    let mut entries = Vec::new();
    for line in headers.get_all(X_FORWARDED_FOR) {
        // A line that is not visible ASCII is malformed: if it were the rightmost, the
        // search would end at it, so everything to its left is unreachable too.
        let Ok(text) = line.to_str() else {
            entries.clear();
            entries.push(None);
            continue;
        };
        entries.extend(text.split(',').map(|entry| Some(entry.trim())));
    }
    for entry in entries.into_iter().rev() {
        let ip = normalize(entry?.parse::<IpAddr>().ok()?);
        if !trusted.contains(ip) {
            return Some(ip);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use axum::http::HeaderValue;
    use proptest::prelude::*;

    use super::*;

    const PROXY: &str = "10.250.0.2";

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn trusted(text: &str) -> TrustedProxies {
        TrustedProxies::parse(text, false).unwrap()
    }

    fn headers(pairs: &[(HeaderName, &[u8])]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(name.clone(), HeaderValue::from_bytes(value).unwrap());
        }
        map
    }

    fn xff(lines: &[&str]) -> HeaderMap {
        let pairs: Vec<(HeaderName, &[u8])> = lines
            .iter()
            .map(|line| (X_FORWARDED_FOR, line.as_bytes()))
            .collect();
        headers(&pairs)
    }

    #[test]
    fn table() {
        let proxies = trusted("10.250.0.2, 10.250.0.3");
        let cases: [(&str, &str, HeaderMap, bool, &str); 17] = [
            // Internal with a valid, invalid, trimmed-needed or repeated X-Client-Ip.
            (
                "internal valid",
                "198.51.100.1",
                headers(&[(X_CLIENT_IP, b"203.0.113.7")]),
                true,
                "203.0.113.7",
            ),
            (
                "internal invalid",
                "198.51.100.1",
                headers(&[(X_CLIENT_IP, b"not-an-ip")]),
                true,
                "198.51.100.1",
            ),
            (
                "internal with spaces is not trimmed",
                "198.51.100.1",
                headers(&[(X_CLIENT_IP, b" 203.0.113.7")]),
                true,
                "198.51.100.1",
            ),
            (
                "internal repeated",
                "198.51.100.1",
                headers(&[(X_CLIENT_IP, b"203.0.113.7"), (X_CLIENT_IP, b"203.0.113.8")]),
                true,
                "198.51.100.1",
            ),
            (
                "internal mapped",
                "198.51.100.1",
                headers(&[(X_CLIENT_IP, b"::ffff:203.0.113.7")]),
                true,
                "203.0.113.7",
            ),
            (
                "internal ignores X-Forwarded-For",
                PROXY,
                xff(&["203.0.113.9"]),
                true,
                PROXY,
            ),
            (
                "trusted one line",
                PROXY,
                xff(&["203.0.113.7"]),
                false,
                "203.0.113.7",
            ),
            (
                "trusted rightmost untrusted",
                PROXY,
                xff(&["198.51.100.66, 203.0.113.7"]),
                false,
                "203.0.113.7",
            ),
            (
                "trusted skips proxies on the right",
                PROXY,
                xff(&["203.0.113.7, 10.250.0.3 ,10.250.0.2"]),
                false,
                "203.0.113.7",
            ),
            (
                "trusted several lines",
                PROXY,
                xff(&["198.51.100.66", "203.0.113.7, 10.250.0.3"]),
                false,
                "203.0.113.7",
            ),
            (
                "malformed in the middle",
                PROXY,
                xff(&["203.0.113.7, unknown, 10.250.0.3"]),
                false,
                PROXY,
            ),
            (
                "empty entry",
                PROXY,
                xff(&["203.0.113.7,,10.250.0.3"]),
                false,
                PROXY,
            ),
            (
                "port, brackets and zone are malformed",
                PROXY,
                xff(&["203.0.113.7:443"]),
                false,
                PROXY,
            ),
            (
                "only proxies",
                PROXY,
                xff(&["10.250.0.3, 10.250.0.2"]),
                false,
                PROXY,
            ),
            ("no header", PROXY, HeaderMap::new(), false, PROXY),
            (
                "untrusted peer ignores X-Forwarded-For",
                "198.51.100.1",
                xff(&["203.0.113.7"]),
                false,
                "198.51.100.1",
            ),
            (
                "mapped in X-Forwarded-For",
                PROXY,
                xff(&["::ffff:203.0.113.7"]),
                false,
                "203.0.113.7",
            ),
        ];
        for (name, peer, map, internal, expected) in cases {
            assert_eq!(
                client_ip(ip(peer), &map, internal, &proxies),
                ip(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn brackets_zones_and_ipv6_entries() {
        let proxies = trusted(PROXY);
        for entry in [
            "[2001:db8::1]",
            "fe80::1%eth0",
            "[2001:db8::1]:443",
            "unknown",
            "",
        ] {
            assert_eq!(
                client_ip(ip(PROXY), &xff(&[entry]), false, &proxies),
                ip(PROXY),
                "{entry:?}"
            );
        }
        assert_eq!(
            client_ip(ip(PROXY), &xff(&["2001:db8::1"]), false, &proxies),
            ip("2001:db8::1")
        );
    }

    #[test]
    fn a_non_ascii_line_is_malformed() {
        let proxies = trusted(PROXY);
        let map = headers(&[
            (X_FORWARDED_FOR, b"203.0.113.7"),
            (X_FORWARDED_FOR, b"\xff"),
        ]);
        assert_eq!(client_ip(ip(PROXY), &map, false, &proxies), ip(PROXY));
        // A non-ASCII line to the left of a valid one does not matter.
        let map = headers(&[
            (X_FORWARDED_FOR, b"\xff"),
            (X_FORWARDED_FOR, b"203.0.113.7"),
        ]);
        assert_eq!(
            client_ip(ip(PROXY), &map, false, &proxies),
            ip("203.0.113.7")
        );
        let map = headers(&[(X_CLIENT_IP, b"\xff")]);
        assert_eq!(client_ip(ip(PROXY), &map, true, &proxies), ip(PROXY));
    }

    #[test]
    fn a_mapped_peer_is_normalized() {
        let proxies = trusted(PROXY);
        assert_eq!(
            client_ip(
                ip("::ffff:198.51.100.1"),
                &HeaderMap::new(),
                false,
                &proxies
            ),
            ip("198.51.100.1")
        );
        // A mapped trusted peer is trusted.
        assert_eq!(
            client_ip(
                ip("::ffff:10.250.0.2"),
                &xff(&["203.0.113.7"]),
                false,
                &proxies
            ),
            ip("203.0.113.7")
        );
    }

    fn arb_ip() -> impl Strategy<Value = IpAddr> {
        prop_oneof![
            // A small IPv4 space so proxies, peers and entries collide often.
            (0u8..4).prop_map(|n| IpAddr::V4(Ipv4Addr::new(10, 250, 0, n))),
            any::<u32>().prop_map(|n| IpAddr::V4(Ipv4Addr::from(n))),
            any::<u128>().prop_map(|n| IpAddr::V6(Ipv6Addr::from(n))),
        ]
    }

    fn arb_entry() -> impl Strategy<Value = String> {
        prop_oneof![
            4 => arb_ip().prop_map(|ip| ip.to_string()),
            1 => Just("unknown".to_owned()),
            1 => Just(String::new()),
            1 => Just("[::1]:80".to_owned()),
        ]
    }

    fn arb_lines() -> impl Strategy<Value = Vec<Vec<String>>> {
        prop::collection::vec(prop::collection::vec(arb_entry(), 1..4), 0..3)
    }

    fn proxy_list(ips: &[IpAddr]) -> TrustedProxies {
        let usable: Vec<String> = ips
            .iter()
            .filter(|ip| TrustedProxies::parse(&ip.to_string(), true).is_ok())
            .map(ToString::to_string)
            .collect();
        TrustedProxies::parse(&usable.join(","), true).unwrap()
    }

    fn map_of(lines: &[Vec<String>]) -> HeaderMap {
        let joined: Vec<String> = lines.iter().map(|line| line.join(", ")).collect();
        let refs: Vec<&str> = joined.iter().map(String::as_str).collect();
        xff(&refs)
    }

    proptest! {
        /// If the result is a proxy, it is the peer and no valid entry outside the proxies
        /// was reachable (008 plan-a, question 4).
        #[test]
        fn a_proxy_result_is_the_peer(
            peer in arb_ip(),
            proxies in prop::collection::vec(arb_ip(), 0..4),
            lines in arb_lines(),
        ) {
            let trusted = proxy_list(&proxies);
            let map = map_of(&lines);
            let result = client_ip(peer, &map, false, &trusted);
            if trusted.contains(result) {
                prop_assert_eq!(result, normalize(peer));
                // Walking from the right, every entry before the first malformed one is a
                // proxy.
                let entries: Vec<&String> = lines.iter().flatten().collect();
                for entry in entries.into_iter().rev() {
                    match entry.trim().parse::<IpAddr>() {
                        Ok(ip) => prop_assert!(trusted.contains(ip)),
                        Err(_) => break,
                    }
                }
            }
        }

        /// With an untrusted peer, the result is the peer.
        #[test]
        fn an_untrusted_peer_is_the_result(
            peer in arb_ip(),
            proxies in prop::collection::vec(arb_ip(), 0..4),
            lines in arb_lines(),
        ) {
            let trusted = proxy_list(&proxies);
            prop_assume!(!trusted.contains(peer));
            let result = client_ip(peer, &map_of(&lines), false, &trusted);
            prop_assert_eq!(result, normalize(peer));
        }
    }
}
