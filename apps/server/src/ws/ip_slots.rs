//! Sockets per key (008 RF-A7, RF-B14): anonymous per `IpKey`, with a session per `IpKey` and
//! per user. Bounded by `WS_MAX_CONNECTIONS`: every key in a map has at least one live socket.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::net::IpKey;

/// Anonymous sockets per `IpKey` (ADR-0008 D6). A constant, not an environment variable.
pub const WS_ANON_PER_IP: u32 = 50;
/// Sockets with a session per `IpKey` (D6); they never use the anonymous slots.
pub const WS_SESSION_PER_IP: u32 = 200;
/// Sockets per user (D6).
pub const WS_PER_USER: u32 = 5;

pub struct Slots<K: Hash + Eq + Clone> {
    limit: u32,
    in_use: Mutex<HashMap<K, u32>>,
}

/// The anonymous slots of phase A.
pub type IpSlots = Slots<IpKey>;

impl<K: Hash + Eq + Clone> Slots<K> {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            in_use: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<K, u32>> {
        self.in_use.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One more socket for `key`, or `None` if it already has `limit`.
    pub fn try_acquire(self: &Arc<Self>, key: K) -> Option<Slot<K>> {
        let mut in_use = self.lock();
        let count = in_use.entry(key.clone()).or_insert(0);
        if *count >= self.limit {
            if *count == 0 {
                in_use.remove(&key);
            }
            return None;
        }
        *count += 1;
        Some(Slot {
            slots: self.clone(),
            key,
        })
    }

    /// Keys with at least one socket.
    pub fn keys(&self) -> usize {
        self.lock().len()
    }
}

/// One socket of a key; released on drop (RAII).
pub struct Slot<K: Hash + Eq + Clone> {
    slots: Arc<Slots<K>>,
    key: K,
}

pub type IpSlot = Slot<IpKey>;

impl<K: Hash + Eq + Clone> Drop for Slot<K> {
    fn drop(&mut self) {
        let mut in_use = self.slots.lock();
        if let Some(count) = in_use.get_mut(&self.key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                in_use.remove(&self.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;

    fn key(text: &str) -> IpKey {
        IpKey::from(text.parse::<IpAddr>().unwrap())
    }

    #[test]
    fn the_limit_is_per_key_and_slots_are_released_on_drop() {
        let slots = Arc::new(IpSlots::new(2));
        let a = slots.try_acquire(key("2001:db8:1:2::a")).unwrap();
        let _b = slots.try_acquire(key("2001:db8:1:2::b")).unwrap();
        assert!(slots.try_acquire(key("2001:db8:1:2::c")).is_none());
        let other = slots.try_acquire(key("2001:db8:1:3::a")).unwrap();
        assert_eq!(slots.keys(), 2);
        drop(a);
        assert!(slots.try_acquire(key("2001:db8:1:2::c")).is_some());
        drop(other);
        assert_eq!(slots.keys(), 1);
    }

    #[test]
    fn a_zero_limit_leaves_no_key_behind() {
        let slots = Arc::new(IpSlots::new(0));
        assert!(slots.try_acquire(key("203.0.113.7")).is_none());
        assert_eq!(slots.keys(), 0);
    }
}
