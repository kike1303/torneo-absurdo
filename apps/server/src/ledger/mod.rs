//! The double-entry ledger (ADR-0005, 008 RF-B2): accounts, entries and postings. Coins only
//! move through entries; there is no balance column and no function updates or deletes rows.

/// An amount of coins. Integer, never floating point (ADR-0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coins(pub i64);
