//! The release of a fight's events (RF40; amendment DP-7): each one at `live_started_at + t`, with
//! no lead. A fixed tick, the same for every fight, computes the time elapsed and asks what is due;
//! it never asks when the next event is, so its rhythm does not depend on the log. This task has no
//! access to the database, so a database that is down stops the settlement, never the events.
//!
//! Nothing here writes a log line: `fights_logs.rs` looks for the fragments of the events as canaries.

use std::sync::Arc;
use std::time::Duration;

use protocol::FightEvent;
use tokio::time::MissedTickBehavior;

use super::FightClock;
use super::sealed::{FinalLog, SealedLog};

/// The period of the release, the same for every fight.
pub const TICK: Duration = Duration::from_millis(50);

/// Where the released events go. The hub of phase B implements it. It must not log the events.
pub trait EventSink: Send + Sync {
    /// A batch of a tick; `from_index` is the index of its first event in the fight.
    fn publish(&self, fight_id: i32, from_index: usize, events: &[FightEvent]);
}

/// The sink until phase B.
pub struct DiscardEvents;

impl EventSink for DiscardEvents {
    fn publish(&self, _fight_id: i32, _from_index: usize, _events: &[FightEvent]) {}
}

/// Releases the fight until its `end`. `None` only if the log says it finished and could not give
/// its final: the runner rebuilds the log, as with a task that dies.
pub async fn run(
    id: i32,
    mut log: SealedLog,
    started_at_ms: i64,
    clock: FightClock,
    sink: Arc<dyn EventSink>,
) -> Option<FinalLog> {
    let stopwatch = clock.stopwatch(started_at_ms);
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let Some(elapsed) = stopwatch.elapsed_ms() else {
            continue;
        };
        let from_index = log.released_count();
        let events = log.release_due(elapsed);
        if !events.is_empty() {
            sink.publish(id, from_index, &events);
        }
        if log.end_released() {
            return log.into_final(started_at_ms).ok();
        }
    }
}
