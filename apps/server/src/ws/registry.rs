//! The sockets of each session, in memory (ADR-0008 D6, 008 RF-B10, RF-B14): when a session is
//! revoked (NOTIFY) or found invalid on reconnect, its sockets close at once. Bounded by
//! `WS_MAX_CONNECTIONS`: only live sockets are registered.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio_util::sync::CancellationToken;

use crate::sessions::SessionId;

#[derive(Default)]
pub struct Registry {
    sockets: Mutex<HashMap<SessionId, HashMap<u64, CancellationToken>>>,
    next: AtomicU64,
    closes: AtomicU64,
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, HashMap<SessionId, HashMap<u64, CancellationToken>>> {
        self.sockets.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a socket of `session`. Its token is cancelled when the session is closed; the
    /// guard unregisters it on drop.
    pub fn register(self: &Arc<Self>, session: SessionId) -> Registration {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        self.lock()
            .entry(session)
            .or_default()
            .insert(id, token.clone());
        Registration {
            registry: self.clone(),
            session,
            id,
            token,
        }
    }

    /// Closes every socket of `session` (a revoked session).
    pub fn close(&self, session: SessionId) {
        self.closes.fetch_add(1, Ordering::Relaxed);
        if let Some(sockets) = self.lock().remove(&session) {
            for token in sockets.values() {
                token.cancel();
            }
        }
    }

    /// How many closes were requested, with or without sockets to close (the race tests wait
    /// on it to know a NOTIFY was handled).
    pub fn closes_requested(&self) -> u64 {
        self.closes.load(Ordering::Relaxed)
    }

    /// The sessions with at least one socket.
    pub fn sessions(&self) -> Vec<SessionId> {
        self.lock().keys().copied().collect()
    }

    /// Sockets with a session.
    pub fn len(&self) -> usize {
        self.lock().values().map(HashMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A registered socket.
pub struct Registration {
    registry: Arc<Registry>,
    session: SessionId,
    id: u64,
    token: CancellationToken,
}

impl Registration {
    pub fn session(&self) -> SessionId {
        self.session
    }

    /// Cancelled when the session is closed.
    pub fn closed(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let mut sockets = self.registry.lock();
        if let Some(of_session) = sockets.get_mut(&self.session) {
            of_session.remove(&self.id);
            if of_session.is_empty() {
                sockets.remove(&self.session);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> SessionId {
        SessionId::parse(&uuid::Uuid::from_u128(n).to_string()).unwrap()
    }

    #[test]
    fn closing_a_session_cancels_only_its_sockets_and_drops_unregister() {
        let registry = Arc::new(Registry::default());
        let a1 = registry.register(id(1));
        let a2 = registry.register(id(1));
        let b = registry.register(id(2));
        assert_eq!(registry.len(), 3);
        registry.close(id(1));
        assert!(a1.closed().is_cancelled() && a2.closed().is_cancelled());
        assert!(!b.closed().is_cancelled());
        assert_eq!(registry.sessions(), [id(2)]);
        drop((a1, a2));
        drop(b);
        assert!(registry.is_empty());
    }
}
