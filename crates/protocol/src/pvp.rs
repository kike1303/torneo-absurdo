//! The player fights (021, phase 2): a search, what it resolved, and the two reads.
//! Contract: no human-readable text and never a user id. A player is named by their
//! `display_name`; a character of the tournament by its slug. [`PvpFightDetail`] is public and
//! carries exactly what `verifyFight` of 011 needs; what a fight gave to each side
//! ([`PvpReward`]) only travels in the private answers.
use serde::{Deserialize, Serialize};

use crate::botarga::BotargaProgress;
use crate::fight::{EndReason, Side};
use crate::fights::TimestampMs;
use crate::seasons::SeasonView;

/// The body of `POST /api/pvp/fights`: a canonical UUID the client keeps for its retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct PvpFightRequest {
    pub client_key: String,
}

/// One side of a player fight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PvpFighter {
    /// The botarga of a user, by the user's current `display_name`.
    Player { display_name: String },
    /// A character of the tournament, scaled to the level of the attacker (021 "Sin rival").
    Tournament { slug: String },
}

/// `GET /api/pvp/fights/{id}` (public) and the fight of [`PvpFightCreated`]. The attacker is always
/// side `a`. `spec_a`, `spec_b` and `log` are the canonical JSON of the engine and `seed` is
/// lowercase hexadecimal: the same bytes as a tournament fight, so the browser re-simulates them.
/// `rating_delta` is the change of the attacker; the defender's, with its own K, is in its private
/// [`PvpReward`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PvpFightDetail {
    pub id: u32,
    pub a: PvpFighter,
    pub b: PvpFighter,
    pub spec_a: String,
    pub spec_b: String,
    pub seed: String,
    pub log: String,
    pub ruleset_version: u16,
    pub winner: Option<Side>,
    pub end_reason: EndReason,
    pub rating_delta: i16,
    pub created_at: TimestampMs,
}

/// Which side of a fight the user was on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PvpRole {
    Attacker,
    Defender,
}

/// What a fight gave to one user: their own change of rating, their XP and their coins (a defender
/// never gets coins), and `factor_pct`, the step of the day of the attacker that scaled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PvpReward {
    pub rating_delta: i16,
    pub xp: u32,
    pub coins: u32,
    pub factor_pct: u8,
}

/// One fight of `GET /api/pvp/fights`, from the side of the session's user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PvpFightSummary {
    pub id: u32,
    pub role: PvpRole,
    pub opponent: PvpFighter,
    pub winner: Option<Side>,
    pub end_reason: EndReason,
    pub reward: PvpReward,
    pub created_at: TimestampMs,
}

/// `GET /api/pvp/fights?before=` (private): the fights of the session's user, as attacker and as
/// defender, the most recent first. `next_before` is the `before` of the next page, `null` on the
/// last one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PvpFightsPage {
    pub fights: Vec<PvpFightSummary>,
    pub next_before: Option<u32>,
}

/// The answer of `POST /api/pvp/fights` (private): the fight, what it gave to the attacker, and
/// where the botarga stands after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PvpFightCreated {
    pub fight: PvpFightDetail,
    pub reward: PvpReward,
    pub progress: BotargaProgress,
}

/// One row of the PvP leaderboard, ranked from 1: the rating of the season (live in the current one,
/// before the soft reset in a closed one) and its rated fights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PvpLeaderboardEntry {
    pub rank: u16,
    pub display_name: String,
    pub rating: u16,
    pub rated_fights: u32,
    pub wins: u32,
    pub losses: u32,
}

/// `GET /api/pvp/leaderboard?season=` (public; 021 "Lectura"). `min_fights` is the rated fights an
/// account needs in the season to appear. The seasons of `GET /api/pvp/seasons` are [`SeasonView`]s:
/// the closed ones and the current one, which is never `closed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PvpLeaderboard {
    pub season: SeasonView,
    pub entries: Vec<PvpLeaderboardEntry>,
    pub min_fights: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail() -> PvpFightDetail {
        PvpFightDetail {
            id: 7,
            a: PvpFighter::Player {
                display_name: "Ana".to_owned(),
            },
            b: PvpFighter::Tournament {
                slug: "rata-electrica".to_owned(),
            },
            spec_a: "{}".to_owned(),
            spec_b: "{}".to_owned(),
            seed: "00".repeat(32),
            log: "{}".to_owned(),
            ruleset_version: 2,
            winner: Some(Side::A),
            end_reason: EndReason::Ko,
            rating_delta: 0,
            created_at: TimestampMs::from_ms(1_700_000_000_000),
        }
    }

    #[test]
    fn a_detail_has_exactly_the_fields_of_the_verification() {
        let json = serde_json::to_value(detail()).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "a",
                "b",
                "created_at",
                "end_reason",
                "id",
                "log",
                "rating_delta",
                "ruleset_version",
                "seed",
                "spec_a",
                "spec_b",
                "winner",
            ]
        );
        assert_eq!(
            json["a"],
            serde_json::json!({"kind": "player", "display_name": "Ana"})
        );
        assert_eq!(
            json["b"],
            serde_json::json!({"kind": "tournament", "slug": "rata-electrica"})
        );
        assert_eq!(json["created_at"], "1700000000000");
        let text = json.to_string();
        for private in ["user", "daily", "coins", "xp", "client_key"] {
            assert!(!text.contains(private), "{private}");
        }
    }

    #[test]
    fn a_request_refuses_what_it_does_not_know() {
        let key = r#"{"client_key":"0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10"}"#;
        assert!(serde_json::from_str::<PvpFightRequest>(key).is_ok());
        for bad in [
            r#"{"client_key":"k","defender":"x"}"#,
            r#"{}"#,
            r#"{"client_key":1}"#,
        ] {
            assert!(
                serde_json::from_str::<PvpFightRequest>(bad).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_leaderboard_entry_names_no_user_id() {
        let board = PvpLeaderboard {
            season: SeasonView {
                id: 202_611,
                starts_at: TimestampMs::from_ms(0),
                ends_at: TimestampMs::from_ms(1),
                closed: false,
            },
            entries: vec![PvpLeaderboardEntry {
                rank: 1,
                display_name: "Ana".to_owned(),
                rating: 1240,
                rated_fights: 12,
                wins: 9,
                losses: 3,
            }],
            min_fights: 10,
        };
        let json = serde_json::to_value(&board).unwrap();
        let entry = json["entries"][0].as_object().unwrap();
        let mut keys: Vec<&str> = entry.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "display_name",
                "losses",
                "rank",
                "rated_fights",
                "rating",
                "wins"
            ]
        );
        assert_eq!(
            serde_json::from_value::<PvpLeaderboard>(json).unwrap(),
            board
        );
    }

    #[test]
    fn a_summary_names_the_role_in_snake_case() {
        let summary = PvpFightSummary {
            id: 1,
            role: PvpRole::Defender,
            opponent: PvpFighter::Player {
                display_name: "Beto".to_owned(),
            },
            winner: None,
            end_reason: EndReason::Draw,
            reward: PvpReward {
                rating_delta: 0,
                xp: 0,
                coins: 0,
                factor_pct: 75,
            },
            created_at: TimestampMs::from_ms(0),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["role"], "defender");
        assert_eq!(json["winner"], serde_json::Value::Null);
        assert_eq!(json["end_reason"], "draw");
        assert_eq!(json["reward"]["factor_pct"], 75);
    }
}
