//! Friends and their challenges (feature 023). Contract: no human-readable text and never a user
//! id. A friend is named by their public `display_name`; the server looks it up only among the
//! friends of the session's user. Everything here is private: it only travels in the answers to
//! the user it belongs to.
use serde::{Deserialize, Serialize};

use crate::fight::{EndReason, Side};
use crate::fights::TimestampMs;
use crate::pvp::{PvpFightDetail, PvpRole};

/// A friend, as the list shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendEntry {
    pub display_name: String,
    pub level: u8,
    /// They can be challenged now: a build that fits the rules of this server and no ban. The list
    /// does not say which one failed.
    pub ready: bool,
}

/// `GET /api/friends`: my friends, by name, and my invite code (`null` until one is made).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendsView {
    pub friends: Vec<FriendEntry>,
    pub invite: Option<String>,
}

/// The body of `POST /api/friends/invite/preview` and `POST /api/friends/accept`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct FriendInviteRequest {
    pub code: String,
}

/// Who an invite is from, before accepting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendInvitePreview {
    pub display_name: String,
}

/// The body of `POST /api/friends/remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct FriendRemoveRequest {
    pub display_name: String,
}

/// The body of `POST /api/friends/challenge`: the friend and a canonical UUID the client keeps for
/// its retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct FriendChallengeRequest {
    pub display_name: String,
    pub client_key: String,
}

/// One fight of `GET /api/friends/fights`, from the side of the session's user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendFightSummary {
    pub id: u32,
    pub role: PvpRole,
    pub opponent: String,
    pub winner: Option<Side>,
    pub end_reason: EndReason,
    /// The tournament of a match of a bracket; `null` for a challenge.
    pub tournament_id: Option<u32>,
    pub created_at: TimestampMs,
}

/// `GET /api/friends/fights?before=`: the newest first; `next_before` is `null` on the last page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendFightsPage {
    pub fights: Vec<FriendFightSummary>,
    pub next_before: Option<u32>,
}

/// The answer of `POST /api/friends/challenge`, and of `GET /api/friends/fights/{id}`: the fight
/// with what `verifyFight` of 011 needs. `rating_delta` is always 0: nothing between friends moves
/// the rating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendFight {
    pub fight: PvpFightDetail,
}

/// The body of `POST /api/friends/tournaments`: 4 or 8, the other `size - 1` friends by name (the
/// owner takes a slot) and a canonical UUID the client keeps for its retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct FriendTournamentRequest {
    pub size: u8,
    pub display_names: Vec<String>,
    pub client_key: String,
}

/// One match of a bracket. Round 1 is the first; `slot` is its place in the round, from the top.
/// `a` is the side of the lower slot. `advances` is the winner, or after a draw the side the low
/// bit of the first byte of the seed of the fight says (0 = `a`), so the browser can check it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BracketMatch {
    pub round: u8,
    pub slot: u8,
    pub a: String,
    pub b: String,
    pub fight_id: u32,
    pub advances: Side,
}

/// The answer of `POST /api/friends/tournaments` and of `GET /api/friends/tournaments/{id}`:
/// the players in the order of the bracket, every match by round and slot, and the champion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendTournament {
    pub id: u32,
    pub size: u8,
    pub owner: String,
    pub players: Vec<String>,
    pub matches: Vec<BracketMatch>,
    pub champion: String,
    pub created_at: TimestampMs,
}

/// One tournament of `GET /api/friends/tournaments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendTournamentSummary {
    pub id: u32,
    pub size: u8,
    pub owner: String,
    pub champion: String,
    pub created_at: TimestampMs,
}

/// `GET /api/friends/tournaments?before=`: the newest first; `next_before` is `null` on the last
/// page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FriendTournamentsPage {
    pub tournaments: Vec<FriendTournamentSummary>,
    pub next_before: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_refuse_what_they_do_not_know() {
        assert!(serde_json::from_str::<FriendInviteRequest>(r#"{"code":"x"}"#).is_ok());
        assert!(serde_json::from_str::<FriendInviteRequest>(r#"{"code":"x","user":1}"#).is_err());
        assert!(
            serde_json::from_str::<FriendChallengeRequest>(
                r#"{"display_name":"Ana","client_key":"k","id":"u"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<FriendRemoveRequest>(r#"{}"#).is_err());
        assert!(
            serde_json::from_str::<FriendTournamentRequest>(
                r#"{"size":4,"display_names":["A","B","C"],"client_key":"k","owner":"u"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn a_view_names_no_user_id() {
        let view = FriendsView {
            friends: vec![FriendEntry {
                display_name: "Ana".to_owned(),
                level: 3,
                ready: true,
            }],
            invite: Some("A".repeat(22)),
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["friends"][0]["display_name"], "Ana");
        let mut keys: Vec<&str> = json["friends"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["display_name", "level", "ready"]);
    }
}
