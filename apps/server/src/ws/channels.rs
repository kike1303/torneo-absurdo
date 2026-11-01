//! The subscriptions of one socket (ADR-0008 D6, 008 RF-B15, 010 RF38): at most 8, only channels of
//! the closed enum, `self` only with a session and a fight only if the hub has it.

use std::collections::HashSet;

use protocol::{Channel, FightId, WsErrorCode};

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

    /// Whether `name` can be opened, and whether it already is, in this order: a channel already open
    /// is fine (RF38); unknown, malformed, `self` without a session or a fight for which `known` says
    /// no is `ws.forbidden_channel`; past the limit, `ws.too_many_subscriptions`. It opens nothing.
    pub fn check(
        &self,
        name: &str,
        has_session: bool,
        known: impl Fn(FightId) -> bool,
    ) -> Result<(Channel, bool), WsErrorCode> {
        let channel = Channel::parse(name).ok_or(WsErrorCode::ForbiddenChannel)?;
        if self.open.contains(&channel) {
            return Ok((channel, true));
        }
        let allowed = match channel {
            Channel::SelfChannel => has_session,
            Channel::Schedule => true,
            Channel::Fight(id) => known(id),
        };
        if !allowed {
            return Err(WsErrorCode::ForbiddenChannel);
        }
        if self.open.len() >= self.limit {
            return Err(WsErrorCode::TooManySubscriptions);
        }
        Ok((channel, false))
    }

    pub fn open(&mut self, channel: Channel) {
        self.open.insert(channel);
    }

    pub fn unsubscribe(&mut self, name: &str) -> Result<Channel, WsErrorCode> {
        let channel = Channel::parse(name).ok_or(WsErrorCode::ForbiddenChannel)?;
        self.open.remove(&channel);
        Ok(channel)
    }

    pub fn contains(&self, channel: Channel) -> bool {
        self.open.contains(&channel)
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use protocol::{Channel, FightId};

    use super::*;

    fn subscribe(
        subs: &mut Subscriptions,
        name: &str,
        has_session: bool,
    ) -> Result<Channel, WsErrorCode> {
        let (channel, _) = subs.check(name, has_session, |id| id.get() < 100)?;
        subs.open(channel);
        Ok(channel)
    }

    #[test]
    fn self_needs_a_session_and_unknown_channels_are_forbidden() {
        let mut anonymous = Subscriptions::default();
        assert_eq!(
            subscribe(&mut anonymous, "self", false),
            Err(WsErrorCode::ForbiddenChannel)
        );
        assert_eq!(
            subscribe(&mut anonymous, "schedule", false),
            Ok(Channel::Schedule)
        );
        let mut with_session = Subscriptions::default();
        for name in ["user:1", "SELF", "", "fight:0", "fight:100", "self "] {
            assert_eq!(
                subscribe(&mut with_session, name, true),
                Err(WsErrorCode::ForbiddenChannel),
                "{name:?}"
            );
        }
        assert_eq!(
            subscribe(&mut with_session, "self", true),
            Ok(Channel::SelfChannel)
        );
        // Idempotent, and it says so.
        assert_eq!(
            with_session.check("self", true, |_| false),
            Ok((Channel::SelfChannel, true))
        );
        assert!(with_session.contains(Channel::SelfChannel));
        assert_eq!(with_session.unsubscribe("self"), Ok(Channel::SelfChannel));
        assert!(!with_session.contains(Channel::SelfChannel));
        assert!(with_session.is_empty());
        assert_eq!(
            with_session.unsubscribe("user:1"),
            Err(WsErrorCode::ForbiddenChannel)
        );
    }

    /// T18: the ninth subscription is refused, with real channels. A fight that is no longer known
    /// is still fine if it is open (amendment DP-B5), and an unknown one is forbidden before the limit
    /// is looked at.
    #[test]
    fn the_subscription_past_the_limit_is_refused() {
        assert_eq!(MAX_SUBSCRIPTIONS, 8);
        let mut subs = Subscriptions::default();
        for id in 1..=8 {
            assert!(subscribe(&mut subs, &format!("fight:{id}"), false).is_ok());
        }
        assert_eq!(
            subscribe(&mut subs, "fight:9", false),
            Err(WsErrorCode::TooManySubscriptions)
        );
        assert_eq!(
            subscribe(&mut subs, "schedule", false),
            Err(WsErrorCode::TooManySubscriptions)
        );
        assert_eq!(
            subscribe(&mut subs, "fight:200", false),
            Err(WsErrorCode::ForbiddenChannel)
        );
        assert_eq!(
            subs.check("fight:1", false, |_| false),
            Ok((Channel::Fight(FightId::new(1).unwrap()), true))
        );
    }
}
