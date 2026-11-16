//! The shop (feature 024). Contract: no human-readable text and never a user id. Items are named
//! by their catalog id (an accessory, a clip code or a skin slug); the client shows their names.
//! Everything here is private: it only travels in the answers to the user it belongs to.
use serde::{Deserialize, Serialize};

use crate::botarga::BotargaView;
use crate::coins::CoinAmount;

/// What an item of the shop is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ShopItemKind {
    /// An accessory of the catalog, with its zero-sum stats.
    Accessory,
    /// An idle or victory clip.
    Cosmetic,
    /// A whole botarga, as a suit: only its look.
    Skin,
}

impl ShopItemKind {
    /// The code of the kind, as the database keeps it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accessory => "accessory",
            Self::Cosmetic => "cosmetic",
            Self::Skin => "skin",
        }
    }
}

/// One item of `GET /api/shop`, from the side of the session's user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ShopItem {
    pub kind: ShopItemKind,
    pub id: String,
    pub price: CoinAmount,
    /// Already owned: bought, won or a choice of an open draw is not this; see `in_draw`.
    pub owned: bool,
    /// One of the choices of the open draw: it is had by choosing it there.
    pub in_draw: bool,
    /// The tier of an accessory; `null` for the rest.
    pub tier: Option<u8>,
    /// The level of the user reaches the tier of the item (always true but for accessories).
    pub level_ok: bool,
}

/// `GET /api/shop`: every item, and the balance of the session's user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ShopView {
    pub items: Vec<ShopItem>,
    pub balance: CoinAmount,
}

/// The body of `POST /api/shop/buy`: the item, the price the client showed (a different current
/// price is refused) and a canonical UUID the client keeps for its retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct ShopBuyRequest {
    pub kind: ShopItemKind,
    pub item_id: String,
    pub expected_price: CoinAmount,
    pub client_key: String,
}

/// The answer of `POST /api/shop/buy`: what was bought and for how much, the balance after it and
/// the botarga with its new unlock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ShopPurchase {
    pub kind: ShopItemKind,
    pub item_id: String,
    pub price: CoinAmount,
    pub balance: CoinAmount,
    pub botarga: BotargaView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_refuses_what_it_does_not_know() {
        let ok = r#"{"kind":"cosmetic","item_id":"idle_2","expected_price":400,"client_key":"k"}"#;
        assert!(serde_json::from_str::<ShopBuyRequest>(ok).is_ok());
        let extra = r#"{"kind":"cosmetic","item_id":"idle_2","expected_price":400,"client_key":"k","user":"u"}"#;
        assert!(serde_json::from_str::<ShopBuyRequest>(extra).is_err());
        let kind = r#"{"kind":"ability","item_id":"x","expected_price":1,"client_key":"k"}"#;
        assert!(serde_json::from_str::<ShopBuyRequest>(kind).is_err());
    }

    #[test]
    fn kinds_have_their_codes() {
        for kind in [
            ShopItemKind::Accessory,
            ShopItemKind::Cosmetic,
            ShopItemKind::Skin,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
        }
    }
}
