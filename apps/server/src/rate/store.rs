//! `CappedStore` (008 plan-a decision 6): governor's `HashMap` store, shared with the
//! limiter so the admission check can ask whether a key exists.

use std::sync::Arc;

use governor::nanos::Nanos;
use governor::state::StateStore;
use governor::state::keyed::{HashMapStateStore, ShrinkableKeyedStateStore};

use crate::net::IpKey;

#[derive(Clone, Default)]
pub struct CappedStore(Arc<HashMapStateStore<IpKey>>);

impl CappedStore {
    pub fn contains(&self, key: &IpKey) -> bool {
        self.0.lock().contains_key(key)
    }

    /// Buckets allocated by the table (the memory bound of decision 11).
    pub fn capacity(&self) -> usize {
        self.0.lock().capacity()
    }
}

impl StateStore for CappedStore {
    type Key = IpKey;

    fn measure_and_replace<T, F, E>(&self, key: &IpKey, f: F) -> Result<T, E>
    where
        F: Fn(Option<Nanos>) -> Result<(T, Nanos), E>,
    {
        self.0.measure_and_replace(key, f)
    }
}

impl ShrinkableKeyedStateStore<IpKey> for CappedStore {
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
