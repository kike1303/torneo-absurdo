//! The `self` channel (ADR-0008 D6, 008 RF-B15): events of one user, delivered only to the
//! sockets of that user that subscribed. Never an id from the client: the hub is keyed by the
//! session's user.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use protocol::SelfEvent;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Events waiting per socket; a socket that falls behind loses them (it can reload `/api/me`).
pub const QUEUE: usize = 16;

type Senders = HashMap<Uuid, HashMap<u64, mpsc::Sender<SelfEvent>>>;

#[derive(Default)]
pub struct SelfHub {
    subscribers: Mutex<Senders>,
    next: AtomicU64,
}

impl SelfHub {
    fn lock(&self) -> MutexGuard<'_, Senders> {
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Subscribes the queue of a socket of `user`; dropping the subscription unsubscribes it.
    pub fn subscribe(
        self: &Arc<Self>,
        user: Uuid,
        sender: mpsc::Sender<SelfEvent>,
    ) -> SelfSubscription {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock().entry(user).or_default().insert(id, sender);
        SelfSubscription {
            hub: self.clone(),
            user,
            id,
        }
    }

    /// Delivers `event` to every subscribed socket of `user`, and to nobody else.
    pub fn publish(&self, user: Uuid, event: SelfEvent) {
        if let Some(sockets) = self.lock().get(&user) {
            for sender in sockets.values() {
                let _ = sender.try_send(event.clone());
            }
        }
    }
}

pub struct SelfSubscription {
    hub: Arc<SelfHub>,
    user: Uuid,
    id: u64,
}

impl Drop for SelfSubscription {
    fn drop(&mut self) {
        let mut subscribers = self.hub.lock();
        if let Some(sockets) = subscribers.get_mut(&self.user) {
            sockets.remove(&self.id);
            if sockets.is_empty() {
                subscribers.remove(&self.user);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn events_reach_only_the_subscribed_sockets_of_the_user() {
        let hub = Arc::new(SelfHub::default());
        let (a_tx, mut a_rx) = mpsc::channel(QUEUE);
        let (b_tx, mut b_rx) = mpsc::channel(QUEUE);
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let a_sub = hub.subscribe(a, a_tx);
        let _b_sub = hub.subscribe(b, b_tx);
        let event = SelfEvent::ProfileChanged {
            display_name: "X".to_owned(),
        };
        hub.publish(a, event.clone());
        assert_eq!(a_rx.try_recv().ok(), Some(event.clone()));
        assert!(b_rx.try_recv().is_err());
        drop(a_sub);
        hub.publish(a, event);
        assert!(a_rx.try_recv().is_err());
        assert_eq!(hub.lock().len(), 1);
    }
}
