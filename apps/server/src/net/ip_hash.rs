//! `IP_HASH_KEY` and `ip_hash` (ADR-0008 D8, 008 RF-B16).

use std::fmt;

/// The HMAC key of `ip_hash`, decoded from base64url. `Debug` never shows it.
#[derive(Clone)]
pub struct IpHashKey(Box<[u8]>);

impl IpHashKey {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes.into_boxed_slice())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for IpHashKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IpHashKey(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_key_bytes() {
        let bytes: Vec<u8> = b"raw-key-canary-raw-key-canary-12".to_vec();
        let shown = format!("{:?}", IpHashKey::new(bytes.clone()));
        assert_eq!(shown, "IpHashKey(..)");
        let list = format!("{bytes:?}");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        for raw in [list.as_str(), &list[1..20], hex.as_str(), "raw-key-canary"] {
            assert!(!shown.contains(raw), "{raw}");
        }
    }
}
