//! `CappedStore` (008 plan-a decision 6): governor's `HashMap` store, shared with the
//! limiter so the admission check can ask whether a key exists.

use std::hash::Hash;
use std::sync::Arc;

use governor::nanos::Nanos;
use governor::state::StateStore;
use governor::state::keyed::{HashMapStateStore, ShrinkableKeyedStateStore};

use crate::net::IpKey;

/// Generic over the key: `IpKey` by default, `Uuid` for the per-user buckets (008 phase B).
pub struct CappedStore<K: Hash + Eq + Clone = IpKey>(Arc<HashMapStateStore<K>>);

impl<K: Hash + Eq + Clone> Clone for CappedStore<K> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<K: Hash + Eq + Clone> Default for CappedStore<K> {
    fn default() -> Self {
        Self(Arc::default())
    }
}

impl<K: Hash + Eq + Clone> CappedStore<K> {
    pub fn contains(&self, key: &K) -> bool {
        self.0.lock().contains_key(key)
    }

    /// Buckets allocated by the table (the memory bound of decision 11).
    pub fn capacity(&self) -> usize {
        self.0.lock().capacity()
    }
}

impl<K: Hash + Eq + Clone> StateStore for CappedStore<K> {
    type Key = K;

    fn measure_and_replace<T, F, E>(&self, key: &K, f: F) -> Result<T, E>
    where
        F: Fn(Option<Nanos>) -> Result<(T, Nanos), E>,
    {
        self.0.measure_and_replace(key, f)
    }
}

impl<K: Hash + Eq + Clone> ShrinkableKeyedStateStore<K> for CappedStore<K> {
    fn retain_recent(&self, drop_below: Nanos) {
        self.0.retain_recent(drop_below);
    }

    fn shrink_to_fit(&self) {
        self.0.shrink_to_fit();
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
