//! The bets (feature 012). In phase A there is no API: only the hook of the settlement, which refunds
//! what it has to and refuses what it cannot pay yet.

pub mod settle;

pub use settle::BetsHook;
