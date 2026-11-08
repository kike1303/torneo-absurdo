//! Bets, the wallet and the pools (012 "Tipos"). Contract: coins are [`CoinAmount`], hours are
//! [`TimestampMs`], no human-readable text, and nothing names another bettor: a pool has sums and
//! counts, never identities (RF34).
use serde::{Deserialize, Serialize};

use crate::coins::CoinAmount;
use crate::fight::Side;
use crate::fights::TimestampMs;

/// `POST /api/bets` (012 RF25). `client_key` is a canonical (lowercase, hyphenated) UUID that
/// makes a retry idempotent; the server checks its form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct BetRequest {
    pub fight_id: u32,
    pub side: Side,
    pub amount: u32,
    pub client_key: String,
}

/// The answer of a bet: the wallet and the bets of that fight, as they are after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BetPlaced {
    pub wallet: Wallet,
    pub mine: MyFightBets,
}

/// What a bailout claim would answer now (012 RF30, RF32).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum BailoutState {
    Available,
    /// Today's grant comes first.
    GrantFirst,
    NotEligible,
    AlreadyClaimed,
}

/// A new account's lower cap, and what is left of it (012 RF27, ADR-0008 notes 2026-11-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Probation {
    pub days_left: u16,
    pub fights_left: u16,
}

/// `GET /api/wallet` (012 RF32): the same facts the `POST` routes decide with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Wallet {
    pub balance: CoinAmount,
    pub open_stakes: CoinAmount,
    pub bonus_available: bool,
    pub bailout: BailoutState,
    pub bonus_amount: CoinAmount,
    pub bailout_amount: CoinAmount,
    pub min_bet: CoinAmount,
    pub fight_cap: CoinAmount,
    pub day_resets_at: TimestampMs,
    pub probation: Option<Probation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum BetOutcome {
    Won,
    Lost,
    Refunded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RefundReason {
    OneSided,
    Draw,
    Voided,
    Cancelled,
}

/// The outcome of a whole pool (`pool_settlements.outcome`, 012 RF9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PoolOutcome {
    Paid,
    RefundOneSided,
    RefundDraw,
    RefundVoided,
    RefundCancelled,
}

impl PoolOutcome {
    pub const ALL: &'static [Self] = &[
        Self::Paid,
        Self::RefundOneSided,
        Self::RefundDraw,
        Self::RefundVoided,
        Self::RefundCancelled,
    ];

    /// The value of `pool_settlements.outcome` (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Paid => "paid",
            Self::RefundOneSided => "refund_one_sided",
            Self::RefundDraw => "refund_draw",
            Self::RefundVoided => "refund_voided",
            Self::RefundCancelled => "refund_cancelled",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|o| o.as_str() == text)
    }

    /// Why the stakes went back, for a refund.
    pub const fn refund_reason(self) -> Option<RefundReason> {
        match self {
            Self::Paid => None,
            Self::RefundOneSided => Some(RefundReason::OneSided),
            Self::RefundDraw => Some(RefundReason::Draw),
            Self::RefundVoided => Some(RefundReason::Voided),
            Self::RefundCancelled => Some(RefundReason::Cancelled),
        }
    }
}

/// One bet of the session's user. `id` is the bet's UUID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BetView {
    pub id: String,
    pub side: Side,
    pub stake: CoinAmount,
    pub placed_at: TimestampMs,
}

/// The result of all the user's bets on a settled fight: one side only (RF26), so one result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MyFightResult {
    pub outcome: BetOutcome,
    pub payout: CoinAmount,
    pub net: CoinAmount,
    pub refund_reason: Option<RefundReason>,
}

/// `GET /api/bets/mine?fight_id=` (012 RF33). Never `ranked_profit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MyFightBets {
    pub fight_id: u32,
    pub side: Option<Side>,
    pub staked: CoinAmount,
    pub max_stake: CoinAmount,
    pub bets: Vec<BetView>,
    pub result: Option<MyFightResult>,
}

/// `GET /api/fights/{id}/quote` (012 RF47): what this amount alone would collect if its side
/// wins, with the current pool plus itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BetQuote {
    pub fight_id: u32,
    pub side: Side,
    pub amount: CoinAmount,
    pub payout: CoinAmount,
    pub net: CoinAmount,
    pub one_sided: bool,
    pub rake_bp: u16,
}

/// One side of a pool. `payout_bp` is what 10 000 coins would collect now; `None` while either
/// side is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PoolSide {
    pub stake: CoinAmount,
    pub bets: u32,
    pub payout_bp: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PoolSettlement {
    pub outcome: PoolOutcome,
    pub rake: CoinAmount,
    pub residue: CoinAmount,
}

/// `GET /api/fights/{id}/pool` and `pool_update` (012 RF34, RF35).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FightPool {
    pub fight_id: u32,
    pub a: PoolSide,
    pub b: PoolSide,
    pub rake_bp: u16,
    pub server_time: TimestampMs,
    pub settlement: Option<PoolSettlement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coins(n: i64) -> CoinAmount {
        CoinAmount::new(n).unwrap()
    }

    fn mine() -> MyFightBets {
        MyFightBets {
            fight_id: 7,
            side: Some(Side::A),
            staked: coins(50),
            max_stake: coins(0),
            bets: vec![BetView {
                id: "1b4e28ba-2fa1-11d2-883f-0016d3cca427".to_owned(),
                side: Side::A,
                stake: coins(50),
                placed_at: TimestampMs::from_ms(5),
            }],
            result: Some(MyFightResult {
                outcome: BetOutcome::Won,
                payout: coins(95),
                net: coins(45),
                refund_reason: None,
            }),
        }
    }

    fn wallet() -> Wallet {
        Wallet {
            balance: coins(450),
            open_stakes: coins(50),
            bonus_available: false,
            bailout: BailoutState::NotEligible,
            bonus_amount: coins(500),
            bailout_amount: coins(250),
            min_bet: coins(25),
            fight_cap: coins(500),
            day_resets_at: TimestampMs::from_ms(86_400_000),
            probation: Some(Probation {
                days_left: 6,
                fights_left: 9,
            }),
        }
    }

    #[test]
    fn bet_request_has_the_exact_shape_and_nothing_else() {
        let json = r#"{"fight_id":7,"side":"a","amount":25,"client_key":"1b4e28ba-2fa1-11d2-883f-0016d3cca427"}"#;
        let body: BetRequest = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&body).unwrap(), json);
        for bad in [
            r#"{"fight_id":7,"side":"a","amount":25,"client_key":"k","user_id":"x"}"#,
            r#"{"fight_id":7,"side":"c","amount":25,"client_key":"k"}"#,
            r#"{"fight_id":-1,"side":"a","amount":25,"client_key":"k"}"#,
            r#"{"fight_id":7,"side":"a","amount":-5,"client_key":"k"}"#,
            r#"{"fight_id":7,"side":"a","client_key":"k"}"#,
        ] {
            assert!(serde_json::from_str::<BetRequest>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn wallet_and_bets_have_the_exact_shape() {
        let wallet_json = r#"{"balance":450,"open_stakes":50,"bonus_available":false,"bailout":"not_eligible","bonus_amount":500,"bailout_amount":250,"min_bet":25,"fight_cap":500,"day_resets_at":"86400000","probation":{"days_left":6,"fights_left":9}}"#;
        assert_eq!(serde_json::to_string(&wallet()).unwrap(), wallet_json);
        let mine_json = r#"{"fight_id":7,"side":"a","staked":50,"max_stake":0,"bets":[{"id":"1b4e28ba-2fa1-11d2-883f-0016d3cca427","side":"a","stake":50,"placed_at":"5"}],"result":{"outcome":"won","payout":95,"net":45,"refund_reason":null}}"#;
        assert_eq!(serde_json::to_string(&mine()).unwrap(), mine_json);
        let placed = BetPlaced {
            wallet: wallet(),
            mine: mine(),
        };
        assert_eq!(
            serde_json::to_string(&placed).unwrap(),
            format!(r#"{{"wallet":{wallet_json},"mine":{mine_json}}}"#)
        );
        assert_eq!(
            serde_json::from_str::<BetPlaced>(&serde_json::to_string(&placed).unwrap()).unwrap(),
            placed
        );
    }

    #[test]
    fn pool_and_quote_have_the_exact_shape_and_no_identities() {
        let pool = FightPool {
            fight_id: 7,
            a: PoolSide {
                stake: coins(12),
                bets: 2,
                payout_bp: Some(17_500),
            },
            b: PoolSide {
                stake: coins(9),
                bets: 1,
                payout_bp: Some(23_333),
            },
            rake_bp: 500,
            server_time: TimestampMs::from_ms(9),
            settlement: Some(PoolSettlement {
                outcome: PoolOutcome::Paid,
                rake: coins(1),
                residue: coins(1),
            }),
        };
        let json = r#"{"fight_id":7,"a":{"stake":12,"bets":2,"payout_bp":17500},"b":{"stake":9,"bets":1,"payout_bp":23333},"rake_bp":500,"server_time":"9","settlement":{"outcome":"paid","rake":1,"residue":1}}"#;
        assert_eq!(serde_json::to_string(&pool).unwrap(), json);
        let quote = BetQuote {
            fight_id: 7,
            side: Side::B,
            amount: coins(100),
            payout: coins(100),
            net: coins(0),
            one_sided: true,
            rake_bp: 0,
        };
        assert_eq!(
            serde_json::to_string(&quote).unwrap(),
            r#"{"fight_id":7,"side":"b","amount":100,"payout":100,"net":0,"one_sided":true,"rake_bp":0}"#
        );
    }

    #[test]
    fn pool_outcomes_round_trip_their_codes() {
        for outcome in PoolOutcome::ALL {
            assert_eq!(PoolOutcome::parse(outcome.as_str()), Some(*outcome));
            assert_eq!(
                serde_json::to_value(outcome).unwrap(),
                serde_json::Value::from(outcome.as_str())
            );
            assert_eq!(
                outcome.refund_reason().is_none(),
                *outcome == PoolOutcome::Paid
            );
        }
        assert_eq!(PoolOutcome::parse("refunded"), None);
    }
}
