use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::me::MAX_SAFE_INTEGER;

/// An amount of coins on the wire (012 area of concern 13). The only 64-bit integer of the
/// protocol: exported as `number`, so it is bounded by `±`[`MAX_SAFE_INTEGER`], and neither
/// [`CoinAmount::new`] nor deserialization builds one outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CoinAmount(#[cfg_attr(feature = "ts", ts(type = "number"))] i64);

/// A coin amount outside `±MAX_SAFE_INTEGER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoinAmountOutOfRange;

impl std::fmt::Display for CoinAmountOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("coin amount out of range")
    }
}

impl CoinAmount {
    pub const ZERO: Self = Self(0);

    /// `None` outside `±MAX_SAFE_INTEGER`: a JavaScript number would lose it.
    #[must_use]
    pub const fn new(coins: i64) -> Option<Self> {
        if coins.unsigned_abs() <= MAX_SAFE_INTEGER.unsigned_abs() {
            Some(Self(coins))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for CoinAmount {
    type Error = CoinAmountOutOfRange;

    fn try_from(coins: i64) -> Result<Self, Self::Error> {
        Self::new(coins).ok_or(CoinAmountOutOfRange)
    }
}

impl Serialize for CoinAmount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for CoinAmount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let coins = i64::deserialize(deserializer)?;
        Self::new(coins).ok_or_else(|| serde::de::Error::custom(CoinAmountOutOfRange))
    }
}

impl From<CoinAmount> for i64 {
    fn from(amount: CoinAmount) -> Self {
        amount.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_exactly_the_safe_range() {
        assert_eq!(CoinAmount::new(0), Some(CoinAmount::ZERO));
        assert_eq!(
            CoinAmount::new(MAX_SAFE_INTEGER).map(CoinAmount::get),
            Some(MAX_SAFE_INTEGER)
        );
        assert_eq!(
            CoinAmount::new(-MAX_SAFE_INTEGER).map(CoinAmount::get),
            Some(-MAX_SAFE_INTEGER)
        );
        assert_eq!(CoinAmount::new(MAX_SAFE_INTEGER + 1), None);
        assert_eq!(CoinAmount::new(-MAX_SAFE_INTEGER - 1), None);
        assert_eq!(CoinAmount::new(i64::MIN), None);
        assert_eq!(CoinAmount::new(i64::MAX), None);
    }

    #[test]
    fn it_travels_as_a_plain_number_and_refuses_one_out_of_range() {
        let amount = CoinAmount::new(-25).unwrap();
        assert_eq!(serde_json::to_string(&amount).unwrap(), "-25");
        assert_eq!(serde_json::from_str::<CoinAmount>("-25").unwrap(), amount);
        let over = (MAX_SAFE_INTEGER + 1).to_string();
        assert!(serde_json::from_str::<CoinAmount>(&over).is_err());
        assert!(serde_json::from_str::<CoinAmount>("\"5\"").is_err());
        assert!(serde_json::from_str::<CoinAmount>("1.5").is_err());
    }
}
