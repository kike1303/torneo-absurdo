//! What the sockets read of the fights (RF38, RF39, RF43): per fight only what was already released,
//! each message serialized once. Nothing here is per socket: a socket keeps its own [`Cursor`], and a
//! `watch` says that something changed, so one that falls behind skips ahead instead of piling up.
//!
//! Nothing here knows an hour or asks when the next event is: the events only come from the release
//! (through [`EventSink`]) and, for a settled fight, from its published log. This module writes no
//! log line.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use axum::extract::ws::Utf8Bytes;
use protocol::{FightEvent, FightState, FightSummary, FightsCurrent, ServerMessage, TimestampMs};
use tokio::sync::watch;

use super::FightClock;
use super::release::EventSink;

/// A shared `fight_sync` or `schedule_update` older than this is serialized again when served: its
/// `server_time` can only delay the sample, and the client keeps the maximum (amendment DP-B4).
pub const SHARED_MAX_AGE_MS: i64 = 250;

/// How long a settled fight that is not `last` stays (amendment DP-B8): a socket sends channel by
/// channel, up to 8 with `close_grace` each, so a slow one that is still alive reads its final.
pub const SETTLED_RETENTION_MS: i64 = 60_000;

/// A message shared by every socket that needs it.
#[derive(Clone)]
struct Cached {
    version: u64,
    released: usize,
    at_ms: i64,
    text: Utf8Bytes,
}

impl Cached {
    fn is_fresh(&self, version: u64, released: usize, now_ms: i64) -> bool {
        self.version == version
            && self.released == released
            && (0..SHARED_MAX_AGE_MS).contains(&now_ms.saturating_sub(self.at_ms))
    }
}

struct FightEntry {
    /// Changes with the summary: unique in the hub, so an entry that comes back is never mistaken.
    version: u64,
    summary: FightSummary,
    /// When its `settled` summary arrived.
    settled_seen_at_ms: Option<i64>,
    released: Vec<FightEvent>,
    /// The last batch: its first index and the message, already serialized.
    last_batch: Option<(usize, Utf8Bytes)>,
    sync: Mutex<Option<Cached>>,
}

impl FightEntry {
    fn sync_text(&self, now_ms: i64) -> Option<Utf8Bytes> {
        let mut cached = self.sync.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(sync) = cached.as_ref()
            && sync.is_fresh(self.version, self.released.len(), now_ms)
        {
            return Some(sync.text.clone());
        }
        let text = encode(&ServerMessage::FightSync {
            fight: Box::new(self.summary.clone()),
            server_time: TimestampMs::from_ms(now_ms),
            events: self.released.clone(),
        })?;
        *cached = Some(Cached {
            version: self.version,
            released: self.released.len(),
            at_ms: now_ms,
            text: text.clone(),
        });
        Some(text)
    }
}

struct Schedule {
    version: u64,
    board: FightsCurrent,
    cached: Option<Cached>,
}

struct HubState {
    next_version: u64,
    schedule: Schedule,
    fights: BTreeMap<i32, FightEntry>,
}

impl HubState {
    const fn version(&mut self) -> u64 {
        self.next_version = self.next_version.saturating_add(1);
        self.next_version
    }
}

/// Where a socket is in a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    version: u64,
    released: usize,
}

/// A settled fight on its way to the hub, with the events of its log if it has one.
pub struct SettledFight {
    pub summary: FightSummary,
    pub events: Option<Arc<[FightEvent]>>,
}

/// What the runner tells the hub about the fights.
#[derive(Default)]
pub struct HubUpdate {
    /// The unsettled ones.
    pub rows: Vec<FightSummary>,
    pub last: Option<SettledFight>,
    /// The settled ones that are not `last`.
    pub finals: Vec<SettledFight>,
    /// What no longer exists as the hub had it, and goes without a word.
    pub gone: Vec<i32>,
}

pub struct FightHub {
    clock: FightClock,
    state: RwLock<HubState>,
    changes: watch::Sender<u64>,
}

fn encode(message: &ServerMessage) -> Option<Utf8Bytes> {
    serde_json::to_string(message).ok().map(Utf8Bytes::from)
}

impl FightHub {
    pub(super) fn new(clock: FightClock) -> Self {
        let board = FightsCurrent {
            server_time: TimestampMs::from_ms(clock.now_ms()),
            halted: None,
            current: None,
            next: None,
            last: None,
        };
        Self {
            clock,
            state: RwLock::new(HubState {
                next_version: 1,
                schedule: Schedule {
                    version: 1,
                    board,
                    cached: None,
                },
                fights: BTreeMap::new(),
            }),
            changes: watch::Sender::new(0),
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, HubState> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, HubState> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }

    fn notify(&self) {
        self.changes
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    /// What wakes a socket: every change, joined if it comes faster than the socket looks.
    pub fn watch(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    /// Whether this fight can be subscribed to. Never asks the database (RF38).
    pub fn knows(&self, id: i32) -> bool {
        self.read().fights.contains_key(&id)
    }

    /// The board changed: every socket on `schedule` gets it.
    pub(super) fn schedule_changed(&self, board: FightsCurrent) {
        {
            let mut state = self.write();
            let version = state.version();
            state.schedule = Schedule {
                version,
                board,
                cached: None,
            };
        }
        self.notify();
    }

    /// The `schedule_update` of a socket that subscribes, and its position.
    pub fn subscribe_schedule(&self) -> (u64, Option<Utf8Bytes>) {
        let mut state = self.write();
        let now_ms = self.clock.now_ms();
        let schedule = &mut state.schedule;
        (schedule.version, Self::schedule_text(schedule, now_ms))
    }

    /// The `schedule_update` this position has not seen, if any.
    pub fn next_schedule(&self, seen: &mut u64) -> Option<Utf8Bytes> {
        if self.read().schedule.version == *seen {
            return None;
        }
        let mut state = self.write();
        let now_ms = self.clock.now_ms();
        let schedule = &mut state.schedule;
        *seen = schedule.version;
        Self::schedule_text(schedule, now_ms)
    }

    fn schedule_text(schedule: &mut Schedule, now_ms: i64) -> Option<Utf8Bytes> {
        if let Some(cached) = &schedule.cached
            && cached.is_fresh(schedule.version, 0, now_ms)
        {
            return Some(cached.text.clone());
        }
        let mut board = schedule.board.clone();
        board.server_time = TimestampMs::from_ms(now_ms);
        let text = encode(&ServerMessage::ScheduleUpdate {
            board: Box::new(board),
        })?;
        schedule.cached = Some(Cached {
            version: schedule.version,
            released: 0,
            at_ms: now_ms,
            text: text.clone(),
        });
        Some(text)
    }

    /// The `fight_sync` of a socket that subscribes, and its position. `None` if the fight is not
    /// here.
    pub fn subscribe_fight(&self, id: i32) -> Option<(Cursor, Option<Utf8Bytes>)> {
        let state = self.read();
        let entry = state.fights.get(&id)?;
        let cursor = Cursor {
            version: entry.version,
            released: entry.released.len(),
        };
        Some((cursor, entry.sync_text(self.clock.now_ms())))
    }

    /// What this position needs of a fight: the last batch if it is exactly behind it, and if not, a
    /// shared `fight_sync`. That is how a slow client skips (RF43).
    pub fn next_for(&self, id: i32, cursor: &mut Cursor) -> Option<Utf8Bytes> {
        let state = self.read();
        let entry = state.fights.get(&id)?;
        if entry.version == cursor.version {
            if entry.released.len() == cursor.released {
                return None;
            }
            if let Some((from, text)) = &entry.last_batch
                && *from == cursor.released
            {
                cursor.released = entry.released.len();
                return Some(text.clone());
            }
        }
        cursor.version = entry.version;
        cursor.released = entry.released.len();
        entry.sync_text(self.clock.now_ms())
    }

    /// The entries still unsettled that are neither in `rows` nor `last`: settled by this runner, by
    /// another one or by the CLI. The hub survives a panic of the loop, so it is the only list.
    pub(super) fn orphans(&self, rows: &BTreeSet<i32>, last: Option<i32>) -> Vec<i32> {
        self.read()
            .fights
            .iter()
            .filter(|(id, entry)| {
                entry.summary.state != FightState::Settled
                    && !rows.contains(id)
                    && last != Some(**id)
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// The fights the runner sees. An entry leaves only when told, or when it is settled, is not
    /// `last` and its retention passed (amendment DP-B8): never for missing from the rows, or its
    /// final would not reach the sockets.
    pub(super) fn update(&self, update: HubUpdate) {
        let now_ms = self.clock.now_ms();
        let mut changed = false;
        {
            let mut state = self.write();
            let last_id = update.last.as_ref().map(|last| last.summary.id);
            for summary in update.rows {
                changed |= Self::upsert(&mut state, summary, None, now_ms);
            }
            for settled in update.last.into_iter().chain(update.finals) {
                // Only a settled summary is a final.
                if settled.summary.state == FightState::Settled {
                    changed |= Self::upsert(&mut state, settled.summary, settled.events, now_ms);
                }
            }
            for id in update.gone {
                state.fights.remove(&id);
            }
            state.fights.retain(|_, entry| {
                let expired = entry
                    .settled_seen_at_ms
                    .is_some_and(|seen| now_ms.saturating_sub(seen) >= SETTLED_RETENTION_MS);
                !expired || last_id == Some(entry.summary.id)
            });
        }
        if changed {
            self.notify();
        }
    }

    fn upsert(
        state: &mut HubState,
        summary: FightSummary,
        events: Option<Arc<[FightEvent]>>,
        now_ms: i64,
    ) -> bool {
        let Ok(id) = i32::try_from(summary.id) else {
            return false;
        };
        let settled = summary.state == FightState::Settled;
        let version = state.version();
        let entry = state.fights.entry(id).or_insert_with(|| FightEntry {
            version: 0,
            summary: summary.clone(),
            settled_seen_at_ms: None,
            released: Vec::new(),
            last_batch: None,
            sync: Mutex::new(None),
        });
        let mut changed = entry.version == 0 || entry.summary != summary;
        // Already public: a settled fight that this process did not see to the end.
        if let Some(events) = events
            && events.len() > entry.released.len()
        {
            entry.released = events.to_vec();
            entry.last_batch = None;
            changed = true;
        }
        if settled {
            entry.settled_seen_at_ms.get_or_insert(now_ms);
        } else {
            entry.settled_seen_at_ms = None;
        }
        if changed {
            entry.summary = summary;
            entry.version = version;
        }
        changed
    }

    /// What the hub holds of a fight, for the tests of memory: the events and the size of the cached
    /// messages. No socket adds to it.
    pub fn held(&self, id: i32) -> Option<(usize, usize)> {
        let state = self.read();
        let entry = state.fights.get(&id)?;
        let batch = entry.last_batch.as_ref().map_or(0, |(_, text)| text.len());
        let sync = entry
            .sync
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or(0, |cached| cached.text.len());
        Some((entry.released.len(), batch.saturating_add(sync)))
    }
}

/// Idempotent: a task rebuilt after dying, or after a panic of the loop, starts over from index 0,
/// and only what the hub does not have counts. A batch that does not fit (an unknown fight, a gap or
/// a fight not `live`) is dropped: the runner tells the hub of a fight before it starts releasing.
impl EventSink for FightHub {
    fn publish(&self, fight_id: i32, from_index: usize, events: &[FightEvent]) {
        {
            let mut state = self.write();
            let Some(entry) = state.fights.get_mut(&fight_id) else {
                return;
            };
            if entry.summary.state != FightState::Live {
                return;
            }
            let have = entry.released.len();
            let Some(skip) = have.checked_sub(from_index) else {
                return;
            };
            let Some(new) = events.get(skip..).filter(|new| !new.is_empty()) else {
                return;
            };
            let Ok(first) = u32::try_from(have) else {
                return;
            };
            let Some(text) = encode(&ServerMessage::FightEvents {
                fight_id: entry.summary.id,
                from_index: first,
                events: new.to_vec(),
                server_time: TimestampMs::from_ms(self.clock.now_ms()),
            }) else {
                return;
            };
            entry.released.extend_from_slice(new);
            entry.last_batch = Some((have, text));
        }
        self.notify();
    }
}

#[cfg(test)]
mod tests {
    use protocol::{Channel, FighterCard};

    use super::super::FightsShared;
    use super::super::board::BoardUpdate;
    use super::*;

    fn summary(id: u32, state: FightState) -> FightSummary {
        let card = |slug: &str| FighterCard {
            slug: slug.to_owned(),
            max_hp: 100,
        };
        FightSummary {
            id,
            state,
            a: card("a"),
            b: card("b"),
            ruleset_version: 1,
            snapshot_sha256: "aa".to_owned(),
            seed_commit: None,
            opens_at: TimestampMs::from_ms(1),
            closes_at: TimestampMs::from_ms(2),
            live_at: TimestampMs::from_ms(3),
            live_started_at: None,
            time_limit_ms: 86_000,
            max_duration_ms: 90_000,
            playback_delay_ms: 3500,
            final_result: match state {
                FightState::Settled => Some(protocol::FightFinal::Cancelled {
                    reason: protocol::CancelReason::MissedWindow,
                }),
                _ => None,
            },
        }
    }

    fn event(t: u32) -> FightEvent {
        FightEvent::Intro { t }
    }

    fn message(text: &Utf8Bytes) -> ServerMessage {
        serde_json::from_str(text.as_str()).unwrap()
    }

    fn rows(hub: &FightHub, fights: Vec<FightSummary>) {
        hub.update(HubUpdate {
            rows: fights,
            ..HubUpdate::default()
        });
    }

    fn settled(summary: FightSummary) -> SettledFight {
        SettledFight {
            summary,
            events: None,
        }
    }

    fn live_hub(clock: &FightClock) -> (FightHub, Cursor) {
        let hub = FightHub::new(clock.clone());
        rows(&hub, vec![summary(1, FightState::Live)]);
        let (cursor, _) = hub.subscribe_fight(1).unwrap();
        (hub, cursor)
    }

    fn the_sync_events(text: &Utf8Bytes) -> Vec<FightEvent> {
        match message(text) {
            ServerMessage::FightSync { events, .. } => events,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_socket_on_time_gets_the_batch_and_a_late_one_a_sync() {
        let clock = FightClock::manual(100);
        let (hub, mut on_time) = live_hub(&clock);
        let mut late = on_time;

        hub.publish(1, 0, &[event(0), event(0)]);
        let batch = hub.next_for(1, &mut on_time).unwrap();
        assert_eq!(
            message(&batch),
            ServerMessage::FightEvents {
                fight_id: 1,
                from_index: 0,
                events: vec![event(0), event(0)],
                server_time: TimestampMs::from_ms(100),
            }
        );
        assert!(hub.next_for(1, &mut on_time).is_none());

        hub.publish(1, 2, &[event(5)]);
        let second = hub.next_for(1, &mut on_time).unwrap();
        assert!(matches!(
            message(&second),
            ServerMessage::FightEvents { from_index: 2, .. }
        ));
        // Behind by two batches: it skips, with everything released.
        let sync = hub.next_for(1, &mut late).unwrap();
        assert_eq!(the_sync_events(&sync), vec![event(0), event(0), event(5)]);
        assert!(hub.next_for(1, &mut late).is_none());
    }

    #[test]
    fn the_same_batch_is_the_same_bytes_and_no_copy() {
        let clock = FightClock::manual(100);
        let (hub, mut a) = live_hub(&clock);
        let mut b = a;
        hub.publish(1, 0, &[event(0)]);
        let (a, b) = (
            hub.next_for(1, &mut a).unwrap(),
            hub.next_for(1, &mut b).unwrap(),
        );
        assert_eq!(a, b);
        assert_eq!(a.as_str().as_ptr(), b.as_str().as_ptr());
    }

    /// R7: a task rebuilt starts over from 0, and nobody gets a batch twice.
    #[test]
    fn publishing_again_from_the_start_adds_only_what_is_new() {
        let clock = FightClock::manual(100);
        let (hub, mut cursor) = live_hub(&clock);
        hub.publish(1, 0, &[event(0), event(1)]);
        let _ = hub.next_for(1, &mut cursor).unwrap();

        let mut changes = hub.watch();
        changes.mark_unchanged();
        hub.publish(1, 0, &[event(0), event(1)]);
        assert!(!changes.has_changed().unwrap(), "nothing new, no word");
        assert!(hub.next_for(1, &mut cursor).is_none());

        hub.publish(1, 0, &[event(0), event(1), event(2)]);
        assert_eq!(
            message(&hub.next_for(1, &mut cursor).unwrap()),
            ServerMessage::FightEvents {
                fight_id: 1,
                from_index: 2,
                events: vec![event(2)],
                server_time: TimestampMs::from_ms(100),
            }
        );
    }

    #[test]
    fn a_batch_that_does_not_fit_is_dropped() {
        let clock = FightClock::manual(100);
        let hub = FightHub::new(clock);
        hub.publish(1, 0, &[event(0)]);
        assert_eq!(hub.held(1), None, "an unknown fight");
        rows(&hub, vec![summary(1, FightState::Live)]);
        hub.publish(1, 3, &[event(0)]);
        assert_eq!(hub.held(1).unwrap().0, 0, "a gap");
        // A second barrier against an error of order: no event outside `live`.
        for state in [
            FightState::Scheduled,
            FightState::BettingOpen,
            FightState::BettingClosed,
            FightState::Settled,
        ] {
            rows(&hub, vec![summary(1, state)]);
            hub.publish(1, 0, &[event(0)]);
            assert_eq!(hub.held(1).unwrap().0, 0, "{state:?}");
        }
    }

    #[test]
    fn a_new_summary_is_a_sync_and_the_same_one_is_nothing() {
        let clock = FightClock::manual(100);
        let hub = FightHub::new(clock);
        rows(&hub, vec![summary(1, FightState::BettingClosed)]);
        let (mut cursor, first) = hub.subscribe_fight(1).unwrap();
        assert_eq!(the_sync_events(&first.unwrap()), vec![]);

        let mut changes = hub.watch();
        changes.mark_unchanged();
        rows(&hub, vec![summary(1, FightState::BettingClosed)]);
        assert!(!changes.has_changed().unwrap());
        assert!(hub.next_for(1, &mut cursor).is_none());

        rows(&hub, vec![summary(1, FightState::Live)]);
        assert!(changes.has_changed().unwrap());
        match message(&hub.next_for(1, &mut cursor).unwrap()) {
            ServerMessage::FightSync { fight, events, .. } => {
                assert_eq!(fight.state, FightState::Live);
                assert!(events.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    /// DP-B4: a shared message keeps its bytes for a moment, and then takes the hour again.
    #[test]
    fn a_shared_sync_is_serialized_again_when_it_gets_old() {
        let clock = FightClock::manual(1000);
        let hub = FightHub::new(clock.clone());
        rows(&hub, vec![summary(1, FightState::Live)]);
        let time = |text: &Utf8Bytes| match message(text) {
            ServerMessage::FightSync { server_time, .. } => server_time.as_ms(),
            other => panic!("{other:?}"),
        };
        let first = hub.subscribe_fight(1).unwrap().1.unwrap();
        assert_eq!(time(&first), Some(1000));

        clock.set(1000 + SHARED_MAX_AGE_MS - 1);
        let same = hub.subscribe_fight(1).unwrap().1.unwrap();
        assert_eq!(first.as_str().as_ptr(), same.as_str().as_ptr());

        clock.set(1000 + SHARED_MAX_AGE_MS);
        let newer = hub.subscribe_fight(1).unwrap().1.unwrap();
        assert_eq!(time(&newer), Some(1000 + SHARED_MAX_AGE_MS));

        // A clock that goes back does not leave it stuck.
        clock.set(10);
        let back = hub.subscribe_fight(1).unwrap().1.unwrap();
        assert_eq!(time(&back), Some(10));
    }

    #[test]
    fn a_cached_sync_follows_its_version_and_what_was_released() {
        let clock = FightClock::manual(1000);
        let (hub, _) = live_hub(&clock);
        let (_, first) = hub.subscribe_fight(1).unwrap();
        hub.publish(1, 0, &[event(0)]);
        let (_, second) = hub.subscribe_fight(1).unwrap();
        assert_eq!(the_sync_events(&first.unwrap()), vec![]);
        assert_eq!(the_sync_events(&second.unwrap()), vec![event(0)]);
        rows(
            &hub,
            vec![
                summary(1, FightState::Live),
                summary(2, FightState::Scheduled),
            ],
        );
        let mut changed = summary(1, FightState::Live);
        changed.live_started_at = Some(TimestampMs::from_ms(7));
        rows(&hub, vec![changed]);
        match message(&hub.subscribe_fight(1).unwrap().1.unwrap()) {
            ServerMessage::FightSync { fight, .. } => {
                assert_eq!(fight.live_started_at, Some(TimestampMs::from_ms(7)));
            }
            other => panic!("{other:?}"),
        }
    }

    /// DP-B8: missing from the rows is not leaving. It waits for its final.
    #[test]
    fn an_orphan_stays_until_its_final_and_a_settled_one_for_its_retention() {
        let clock = FightClock::manual(1000);
        let hub = FightHub::new(clock.clone());
        rows(
            &hub,
            vec![
                summary(1, FightState::Live),
                summary(2, FightState::Scheduled),
            ],
        );
        let (mut cursor, _) = hub.subscribe_fight(1).unwrap();

        // Fight 2 is settled and is `last`, and fight 1 went from the rows without a final yet.
        hub.update(HubUpdate {
            last: Some(settled(summary(2, FightState::Settled))),
            ..HubUpdate::default()
        });
        assert!(hub.knows(1));
        let known = BTreeSet::new();
        assert_eq!(hub.orphans(&known, Some(2)), vec![1]);

        // Its final arrives without ever being `last`, and it reaches the socket.
        hub.update(HubUpdate {
            last: Some(settled(summary(2, FightState::Settled))),
            finals: vec![settled(summary(1, FightState::Settled))],
            ..HubUpdate::default()
        });
        assert_eq!(hub.orphans(&known, Some(2)), Vec::<i32>::new());
        match message(&hub.next_for(1, &mut cursor).unwrap()) {
            ServerMessage::FightSync { fight, .. } => assert_eq!(fight.state, FightState::Settled),
            other => panic!("{other:?}"),
        }

        // One rule: it is not `last` and the time passed.
        clock.advance(SETTLED_RETENTION_MS - 1);
        hub.update(HubUpdate {
            last: Some(settled(summary(2, FightState::Settled))),
            ..HubUpdate::default()
        });
        assert!(hub.knows(1));
        clock.advance(1);
        hub.update(HubUpdate {
            last: Some(settled(summary(2, FightState::Settled))),
            ..HubUpdate::default()
        });
        assert!(!hub.knows(1));
        assert!(hub.knows(2), "`last` stays, whatever its age");
    }

    /// DP-B8: only a settled summary is a final.
    #[test]
    fn a_final_that_is_not_settled_is_ignored_and_gone_goes_in_silence() {
        let clock = FightClock::manual(1000);
        let hub = FightHub::new(clock);
        rows(&hub, vec![summary(1, FightState::Live)]);
        let (mut cursor, _) = hub.subscribe_fight(1).unwrap();
        hub.update(HubUpdate {
            finals: vec![settled(summary(1, FightState::BettingOpen))],
            ..HubUpdate::default()
        });
        assert!(hub.next_for(1, &mut cursor).is_none());
        hub.update(HubUpdate {
            gone: vec![1],
            ..HubUpdate::default()
        });
        assert!(!hub.knows(1));
        assert!(hub.next_for(1, &mut cursor).is_none());
    }

    /// RF38: a fight this process did not see to the end takes the events of its log.
    #[test]
    fn a_settled_fight_takes_its_events_from_the_log_if_it_has_fewer() {
        let clock = FightClock::manual(1000);
        let (hub, _) = live_hub(&clock);
        hub.publish(1, 0, &[event(0)]);
        let log: Arc<[FightEvent]> = vec![event(0), event(1), event(2)].into();
        hub.update(HubUpdate {
            last: Some(SettledFight {
                summary: summary(1, FightState::Settled),
                events: Some(log),
            }),
            ..HubUpdate::default()
        });
        assert_eq!(hub.held(1).unwrap().0, 3);
        let shorter: Arc<[FightEvent]> = vec![event(0)].into();
        hub.update(HubUpdate {
            last: Some(SettledFight {
                summary: summary(1, FightState::Settled),
                events: Some(shorter),
            }),
            ..HubUpdate::default()
        });
        assert_eq!(hub.held(1).unwrap().0, 3);
    }

    /// An entry that comes back is never taken for what a socket had.
    #[test]
    fn versions_do_not_repeat() {
        let clock = FightClock::manual(1000);
        let hub = FightHub::new(clock);
        rows(&hub, vec![summary(1, FightState::Live)]);
        let (mut cursor, _) = hub.subscribe_fight(1).unwrap();
        hub.update(HubUpdate {
            gone: vec![1],
            ..HubUpdate::default()
        });
        rows(&hub, vec![summary(1, FightState::Live)]);
        assert!(hub.next_for(1, &mut cursor).is_some());
    }

    fn schedule_of(shared: &FightsShared, seen: &mut u64) -> Option<ServerMessage> {
        shared.hub().next_schedule(seen).map(|text| message(&text))
    }

    fn halted_of(update: Option<ServerMessage>) -> Option<bool> {
        match update {
            Some(ServerMessage::ScheduleUpdate { board }) => board.halted,
            other => panic!("{other:?}"),
        }
    }

    /// DP-B3, one case per method: `halted` also changes without a pass of the runner.
    #[test]
    fn publish_lock_taken_lock_lost_and_the_first_evaluation_all_announce() {
        let shared = FightsShared::new(FightClock::manual(5));
        let (mut seen, first) = shared.hub().subscribe_schedule();
        assert!(matches!(
            message(&first.unwrap()),
            ServerMessage::ScheduleUpdate { .. }
        ));

        // Taking the lock alone says nothing new: still unknown.
        shared.lock_taken();
        assert!(schedule_of(&shared, &mut seen).is_none());

        // From unknown to `false`, with no pass in between.
        shared.initial_evaluation_done();
        assert_eq!(halted_of(schedule_of(&shared, &mut seen)), Some(false));

        // `publish`, and no message if nothing changes.
        shared.publish(BoardUpdate {
            halted: true,
            ..BoardUpdate::default()
        });
        assert_eq!(halted_of(schedule_of(&shared, &mut seen)), Some(true));
        shared.publish(BoardUpdate {
            halted: true,
            ..BoardUpdate::default()
        });
        assert!(schedule_of(&shared, &mut seen).is_none());

        // Losing the lock, also what the runner does after a panic.
        shared.lock_lost();
        assert_eq!(halted_of(schedule_of(&shared, &mut seen)), None);

        // Taking it back after an evaluation that stayed done is the case `lock_taken` announces.
        shared.initial_evaluation_done();
        assert!(
            schedule_of(&shared, &mut seen).is_none(),
            "without the lock"
        );
        shared.lock_taken();
        assert_eq!(halted_of(schedule_of(&shared, &mut seen)), Some(true));
    }

    #[test]
    fn the_hour_alone_is_no_change() {
        let clock = FightClock::manual(5);
        let shared = FightsShared::new(clock.clone());
        let (mut seen, _) = shared.hub().subscribe_schedule();
        clock.advance(10_000);
        shared.publish(BoardUpdate::default());
        assert!(schedule_of(&shared, &mut seen).is_none());
    }

    #[test]
    fn a_channel_is_what_the_hub_is_asked_for() {
        // The ids of the protocol fit in the column of the table.
        let id = protocol::FightId::new(2_147_483_647).unwrap();
        assert_eq!(Channel::Fight(id).to_string(), "fight:2147483647");
        assert!(i32::try_from(id.get()).is_ok());
    }
}
