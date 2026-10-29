//! `IpKey` (008 RF-A2): the full IPv4 or the `/64` prefix of an IPv6, as an opaque key.

use std::fmt;
use std::net::IpAddr;

/// Rate-limit and per-IP key. Never a `String`, no `Display`, no `Serialize` and no access to
/// the bytes outside the crate; `Debug` prints `IpKey(..)` (ADR-0012 A4.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct IpKey(Repr);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Repr {
    V4([u8; 4]),
    /// The first 8 bytes (the `/64`).
    V6([u8; 8]),
}

/// IPv4-mapped IPv6 (`::ffff:a.b.c.d`) becomes IPv4. `::a.b.c.d` (compatible) does not.
pub fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

impl From<IpAddr> for IpKey {
    fn from(ip: IpAddr) -> Self {
        match normalize(ip) {
            IpAddr::V4(v4) => Self(Repr::V4(v4.octets())),
            IpAddr::V6(v6) => {
                let octets = v6.octets();
                let mut prefix = [0u8; 8];
                prefix.copy_from_slice(&octets[..8]);
                Self(Repr::V6(prefix))
            }
        }
    }
}

impl fmt::Debug for IpKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IpKey(..)")
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use proptest::prelude::*;

    use super::*;

    fn key(text: &str) -> IpKey {
        IpKey::from(text.parse::<IpAddr>().unwrap())
    }

    #[test]
    fn two_ipv6_of_the_same_64_share_the_key() {
        assert_eq!(key("2001:db8:1:2::a"), key("2001:db8:1:2:ffff::1"));
        assert_ne!(key("2001:db8:1:2::a"), key("2001:db8:1:3::a"));
    }

    #[test]
    fn ipv4_is_the_whole_address() {
        assert_ne!(key("203.0.113.7"), key("203.0.113.8"));
        assert_eq!(key("203.0.113.7"), key("203.0.113.7"));
    }

    #[test]
    fn a_mapped_ipv4_is_the_same_key_as_the_ipv4() {
        assert_eq!(key("::ffff:203.0.113.7"), key("203.0.113.7"));
        // `::a.b.c.d` (IPv4-compatible) is not mapped: it stays IPv6.
        assert_ne!(key("::203.0.113.7"), key("203.0.113.7"));
    }

    #[test]
    fn debug_never_shows_the_address() {
        for text in ["203.0.113.7", "2001:db8:1:2::a", "::ffff:198.51.100.9"] {
            let shown = format!("{:?}", key(text));
            assert_eq!(shown, "IpKey(..)");
            for digit in ['0', '1', '2', '3', '7', '8', '9'] {
                assert!(!shown.contains(digit), "{shown}");
            }
        }
    }

    #[test]
    fn normalize_only_unmaps_mapped_addresses() {
        assert_eq!(
            normalize("::ffff:10.0.0.1".parse().unwrap()),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
        );
        assert_eq!(
            normalize(IpAddr::V6(Ipv6Addr::LOCALHOST)),
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        );
    }

    proptest! {
        #[test]
        fn the_key_is_exactly_the_64_prefix(a in any::<u128>(), b in any::<u128>()) {
            let left = IpKey::from(IpAddr::V6(Ipv6Addr::from(a)));
            // Same high 64 bits, any low 64 bits.
            let same = (a & !u128::from(u64::MAX)) | (b & u128::from(u64::MAX));
            let mapped = |x: u128| Ipv6Addr::from(x).to_ipv4_mapped().is_some();
            prop_assume!(!mapped(a) && !mapped(same) && !mapped(b));
            prop_assert_eq!(left, IpKey::from(IpAddr::V6(Ipv6Addr::from(same))));
            let other = IpKey::from(IpAddr::V6(Ipv6Addr::from(b)));
            prop_assert_eq!(left == other, (a >> 64) == (b >> 64));
        }
    }
}
