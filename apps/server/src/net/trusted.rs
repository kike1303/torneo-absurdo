//! `TRUSTED_PROXIES` (008 RF-A3): exact proxy addresses whose `X-Forwarded-For` is read.

use std::fmt;
use std::net::IpAddr;

use super::ip_key::normalize;

/// The proxies, normalized (IPv4-mapped IPv6 becomes IPv4). Only exact addresses: a `/32`
/// or `/128`, so there is no CIDR arithmetic. `Debug` shows only how many there are (they
/// are IPs too, ADR-0012 A4.1).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct TrustedProxies(Vec<IpAddr>);

/// `TRUSTED_PROXIES` is invalid. Carries nothing: the value is never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid trusted proxy list")]
pub struct InvalidTrustedProxies;

impl TrustedProxies {
    /// Parses a comma-separated list of `ip`, `ip/32` (IPv4) or `ip/128` (IPv6), with spaces
    /// allowed around each entry. The empty string is the empty list. Rejects any other
    /// prefix, an empty entry, the unspecified, multicast and broadcast addresses, and the
    /// loopback unless `loopback_origin` (`PUBLIC_ORIGIN` is `http` on loopback).
    pub fn parse(text: &str, loopback_origin: bool) -> Result<Self, InvalidTrustedProxies> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        text.split(',')
            .map(|entry| parse_entry(entry.trim(), loopback_origin))
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    /// Whether `ip` (normalized) is one of the proxies.
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.0.contains(&normalize(ip))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for TrustedProxies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TrustedProxies({} entries)", self.0.len())
    }
}

fn parse_entry(entry: &str, loopback_origin: bool) -> Result<IpAddr, InvalidTrustedProxies> {
    let (address, prefix) = match entry.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (entry, None),
    };
    let ip: IpAddr = address.parse().map_err(|_| InvalidTrustedProxies)?;
    // Exactly the host prefix of its family, as written (`/032` is rejected).
    let host_prefix = if ip.is_ipv4() { "32" } else { "128" };
    if prefix.is_some_and(|prefix| prefix != host_prefix) {
        return Err(InvalidTrustedProxies);
    }
    let ip = normalize(ip);
    let refused = match ip {
        IpAddr::V4(v4) => v4.is_unspecified() || v4.is_multicast() || v4.is_broadcast(),
        IpAddr::V6(v6) => v6.is_unspecified() || v6.is_multicast(),
    };
    if refused || (ip.is_loopback() && !loopback_origin) {
        return Err(InvalidTrustedProxies);
    }
    Ok(ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> TrustedProxies {
        TrustedProxies::parse(text, false).unwrap()
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn empty_is_the_empty_list() {
        assert!(ok("").is_empty());
        assert!(ok("  ").is_empty());
    }

    #[test]
    fn exact_addresses_with_or_without_the_host_prefix() {
        let list = ok("10.250.0.2/32, 10.250.0.3 ,2001:db8::1/128,2001:db8::2");
        assert_eq!(list.len(), 4);
        for text in ["10.250.0.2", "10.250.0.3", "2001:db8::1", "2001:db8::2"] {
            assert!(list.contains(ip(text)), "{text}");
        }
        assert!(!list.contains(ip("10.250.0.4")));
    }

    #[test]
    fn mapped_addresses_are_normalized_on_both_sides() {
        let list = ok("::ffff:10.250.0.2/128");
        assert!(list.contains(ip("10.250.0.2")));
        assert!(list.contains(ip("::ffff:10.250.0.2")));
        let list = ok("10.250.0.2");
        assert!(list.contains(ip("::ffff:10.250.0.2")));
    }

    #[test]
    fn other_prefixes_and_malformed_entries_are_rejected() {
        for text in [
            "10.250.0.0/24",
            "2001:db8::/64",
            "10.250.0.2/0",
            "10.250.0.2/032",
            "10.250.0.2/33",
            "10.250.0.2/128",
            "2001:db8::1/32",
            "2001:db8::1/0128",
            "10.250.0.2/",
            "10.250.0.2,,10.250.0.3",
            "10.250.0.2,",
            "proxy",
            "[2001:db8::1]",
            "10.250.0.2:80",
            "fe80::1%eth0",
            "0.0.0.0",
            "::",
            "224.0.0.1",
            "ff02::1",
            "255.255.255.255",
        ] {
            assert!(TrustedProxies::parse(text, true).is_err(), "{text:?}");
        }
    }

    #[test]
    fn loopback_only_with_a_loopback_origin() {
        for text in [
            "127.0.0.1/32",
            "127.0.0.2",
            "::1",
            "::1/128",
            "::ffff:127.0.0.1",
        ] {
            assert!(TrustedProxies::parse(text, false).is_err(), "{text}");
            assert!(TrustedProxies::parse(text, true).is_ok(), "{text}");
        }
    }

    #[test]
    fn debug_shows_only_the_count() {
        let shown = format!("{:?}", ok("10.250.0.2, 2001:db8::1"));
        assert_eq!(shown, "TrustedProxies(2 entries)");
    }
}
