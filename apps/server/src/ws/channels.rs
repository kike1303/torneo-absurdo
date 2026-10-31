//! The subscriptions of one socket (ADR-0008 D6, 008 RF-B15): at most 8, only channels of the
//! closed enum, and `self` only with a session.

use std::collections::HashSet;

use protocol::{Channel, WsErrorCode};

/// Simultaneous subscriptions per socket (D6).
pub const MAX_SUBSCRIPTIONS: usize = 8;

#[derive(Debug)]
pub struct Subscriptions {
    open: HashSet<Channel>,
    limit: usize,
}

impl Default for Subscriptions {
    fn default() -> Self {
        Self::with_limit(MAX_SUBSCRIPTIONS)
    }
}

impl Subscriptions {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            open: HashSet::new(),
            limit,
        }
    }

    /// Opens `name` (idempotent). Unknown, malformed or with an id: `ws.forbidden_channel`, and
    /// so is `self` without a session. Past the limit: `ws.too_many_subscriptions`.
    pub fn subscribe(&mut self, name: &str, has_session: bool) -> Result<Channel, WsErrorCode> {
        let channel = Channel::parse(name).ok_or(WsErrorCode::ForbiddenChannel)?;
        // Until the hub exists, the only channel that can be opened.
        if channel != Channel::SelfChannel || !has_session {
            return Err(WsErrorCode::ForbiddenChannel);
        }
        if !self.open.contains(&channel) && self.open.len() >= self.limit {
            return Err(WsErrorCode::TooManySubscriptions);
        }
        self.open.insert(channel);
        Ok(channel)
    }

    pub fn unsubscribe(&mut self, name: &str) -> Result<Channel, WsErrorCode> {
        let channel = Channel::parse(name).ok_or(WsErrorCode::ForbiddenChannel)?;
        self.open.remove(&channel);
        Ok(channel)
    }

    pub fn contains(&self, channel: Channel) -> bool {
        self.open.contains(&channel)
    }
}

#[cfg(test)]
mod tests {
    use protocol::Channel;

    use super::*;

    #[test]
    fn self_needs_a_session_and_unknown_channels_are_forbidden() {
        let mut anonymous = Subscriptions::default();
        assert_eq!(
            anonymous.subscribe("self", false),
            Err(WsErrorCode::ForbiddenChannel)
        );
        let mut with_session = Subscriptions::default();
        for name in ["user:1", "SELF", "", "fight:1", "self "] {
            assert_eq!(
                with_session.subscribe(name, true),
                Err(WsErrorCode::ForbiddenChannel),
                "{name:?}"
            );
        }
        assert_eq!(
            with_session.subscribe("self", true),
            Ok(Channel::SelfChannel)
        );
        // Idempotent.
        assert_eq!(
            with_session.subscribe("self", true),
            Ok(Channel::SelfChannel)
        );
        assert!(with_session.contains(Channel::SelfChannel));
        assert_eq!(with_session.unsubscribe("self"), Ok(Channel::SelfChannel));
        assert!(!with_session.contains(Channel::SelfChannel));
        assert_eq!(
            with_session.unsubscribe("user:1"),
            Err(WsErrorCode::ForbiddenChannel)
        );
    }

    /// T18: the subscription past the limit is refused. Phase B has a single channel, so the
    /// limit of 8 is checked here with a limit of 0; the real ninth arrives with TA-13.
    #[test]
    fn the_subscription_past_the_limit_is_refused() {
        assert_eq!(MAX_SUBSCRIPTIONS, 8);
        let mut subs = Subscriptions::with_limit(0);
        assert_eq!(
            subs.subscribe("self", true),
            Err(WsErrorCode::TooManySubscriptions)
        );
        // An unknown channel is forbidden before the limit is looked at.
        assert_eq!(
            subs.subscribe("nope", true),
            Err(WsErrorCode::ForbiddenChannel)
        );
    }
}
