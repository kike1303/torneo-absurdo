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
