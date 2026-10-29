//! Wire types shared by the server (`apps/server`) and the web client (`@torneo/protocol`),
//! including the fight log that the engine (`crates/engine`) produces (`fight`, 003 RF1).
//!
//! Contract (RF-B3, RF-B4):
//! - Pure data: no I/O, no async runtime, no clock.
//! - No human-readable text: only codes (`domain.reason`) that the client maps to i18n keys.
//! - No 64-bit (or wider) integers in exported types: `ts-rs` maps them to `bigint` and
//!   JavaScript numbers lose precision above 2^53. `just gen` fails if one slips in. The single
//!   declared exception is `Me.balance`, exported as `number` and bounded by
//!   [`MAX_SAFE_INTEGER`] on the server (a test keeps it the only one).
//! - Enums with data use `#[serde(tag = "...", rename_all = "snake_case")]`; code enums use
//!   `#[serde(rename = "domain.reason")]` on every variant, and each code enum owns one prefix.
//!   Value enums of the fight log (`Side`, `AbilityKind`, ...) are `rename_all = "snake_case"`.
//!
//! TypeScript bindings are generated with the `ts` feature (`just gen`); the server never
//! enables it.
#![forbid(unsafe_code)]

mod error;
mod fight;
mod health;
mod me;
mod ws;

pub use error::{ApiError, ApiErrorCode, AuthErrorCode, CallbackErrorCode, ErrorCode, WsErrorCode};
pub use fight::{AbilityKind, EndReason, FightEvent, FightLog, FighterInfo, MomentKind, Side};
pub use health::HealthResponse;
pub use me::{Locale, MAX_SAFE_INTEGER, Me, Provider, Role, Theme};
pub use ws::{Channel, ClientMessage, SelfEvent, ServerMessage};
