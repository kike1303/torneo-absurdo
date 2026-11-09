//! Seasons and the leaderboard of bettors (020). Contract: coins are [`CoinAmount`], hours are
//! [`TimestampMs`], no human-readable text, and an entry names a user only by its public
//! `display_name`: never a user id.
use serde::{Deserialize, Serialize};

use crate::coins::CoinAmount;
use crate::fights::TimestampMs;

/// A season: a calendar month in UTC. `id` is `YYYYMM`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SeasonView {
    pub id: u32,
    pub starts_at: TimestampMs,
    pub ends_at: TimestampMs,
    pub closed: bool,
}

/// One row of a leaderboard, ranked from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LeaderboardEntry {
    pub rank: u16,
    pub display_name: String,
    pub ranked_profit: CoinAmount,
    pub settled_fights: u32,
}

/// `GET /api/leaderboard?season=` (020). `min_fights` is what an account needs won or lost in the
/// season to appear, out of probation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Leaderboard {
    pub season: SeasonView,
    pub entries: Vec<LeaderboardEntry>,
    pub min_fights: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leaderboard_has_the_exact_shape_and_no_user_id() {
        let board = Leaderboard {
            season: SeasonView {
                id: 202_611,
                starts_at: TimestampMs::from_ms(1),
                ends_at: TimestampMs::from_ms(2),
                closed: false,
            },
            entries: vec![LeaderboardEntry {
                rank: 1,
                display_name: "Ana".to_owned(),
                ranked_profit: CoinAmount::new(-40).unwrap(),
                settled_fights: 12,
            }],
            min_fights: 10,
        };
        let json = concat!(
            r#"{"season":{"id":202611,"starts_at":"1","ends_at":"2","closed":false},"#,
            r#""entries":[{"rank":1,"display_name":"Ana","ranked_profit":-40,"#,
            r#""settled_fights":12}],"min_fights":10}"#,
        );
        assert_eq!(serde_json::to_string(&board).unwrap(), json);
        assert_eq!(serde_json::from_str::<Leaderboard>(json).unwrap(), board);
        assert!(!json.contains("user"));
    }
}
