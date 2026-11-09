//! Wire types shared by the server (`apps/server`) and the web client (`@torneo/protocol`),
//! including the fight log that the engine (`crates/engine`) produces (`fight`, 003 RF1).
//!
//! Contract (RF-B3, RF-B4):
//! - Pure data: no I/O, no async runtime, no clock.
//! - No human-readable text: only codes (`domain.reason`) that the client maps to i18n keys.
//! - No 64-bit (or wider) integers in exported types: `ts-rs` maps them to `bigint` and
//!   JavaScript numbers lose precision above 2^53. `just gen` fails if one slips in. The single
//!   declared exception is [`CoinAmount`], exported as `number` and bounded by
//!   [`MAX_SAFE_INTEGER`] when it is built (a test keeps it the only one).
//! - Enums with data use `#[serde(tag = "...", rename_all = "snake_case")]`; code enums use
//!   `#[serde(rename = "domain.reason")]` on every variant, and each code enum owns one prefix.
//!   Value enums of the fight log (`Side`, `AbilityKind`, ...) are `rename_all = "snake_case"`.
//!
//! TypeScript bindings are generated with the `ts` feature (`just gen`); the server never
//! enables it.
#![forbid(unsafe_code)]

mod bets;
mod coins;
mod error;
mod fight;
mod fights;
mod health;
mod me;
mod ws;

pub use bets::{
    BailoutState, BetHistoryItem, BetHistoryPage, BetOutcome, BetPlaced, BetQuote, BetRequest,
    BetView, FightPool, MyFightBets, MyFightResult, PoolOutcome, PoolSettlement, PoolSide,
    Probation, RefundReason, Wallet,
};
pub use coins::{CoinAmount, CoinAmountOutOfRange};
pub use error::{
    ApiError, ApiErrorCode, AuthErrorCode, BailoutErrorCode, BetErrorCode, BonusErrorCode,
    CallbackErrorCode, ErrorCode, FightErrorCode, ProfileErrorCode, WalletErrorCode, WsErrorCode,
};
pub use fight::{AbilityKind, EndReason, FightEvent, FightLog, FighterInfo, MomentKind, Side};
pub use fights::{
    CancelReason, FightDetail, FightFinal, FightState, FightSummary, FighterCard, FightsCurrent,
    FightsPage, TimestampMs, VoidReason, VoidStats,
};
pub use health::HealthResponse;
pub use me::{
    DisplayNameRequest, Locale, MAX_SAFE_INTEGER, Me, PreferencesRequest, Provider, ReauthStatus,
    Role, Theme, UnlinkStatus,
};
pub use ws::{Channel, ClientMessage, FightId, SelfEvent, ServerMessage};
