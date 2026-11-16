//! The shop (feature 024): cosmetics bought with the coins of the bets. Only looks and zero-sum
//! accessories; never an ability, a slot, points, XP or levels. A purchase is one transaction: the
//! row of `shop_purchases`, its `shop_purchase` entry of the ledger, the unlock and the event. The
//! database ties them (TA008, TA015, TA016); this module only decides what may be bought.

pub mod api;
pub mod store;

use protocol::{IdleClip, ShopItemKind, VictoryClip};
use serde::Deserialize;

use crate::botarga::BotargaCatalog;

/// The highest price of an item (the `CHECK` of `shop_purchases.price`).
pub const PRICE_MAX: u32 = 1_000_000;

/// One item of `content/catalog/shop.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: ShopItemKind,
    pub id: String,
    pub price: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItem {
    kind: ShopItemKind,
    id: String,
    price: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalog {
    items: Vec<RawItem>,
}

/// Why the catalog of the shop cannot be used, by code.
#[derive(Debug, thiserror::Error)]
#[error("the shop catalog is not valid: {code}")]
pub struct ShopCatalogError {
    pub code: &'static str,
}

const fn invalid(code: &'static str) -> ShopCatalogError {
    ShopCatalogError { code }
}

/// Whether `id` has the form of an item id (the `CHECK` of `shop_purchases.item_id`).
#[must_use]
pub fn well_formed(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// The catalog of the shop, checked against the botarga catalog.
#[derive(Debug, Clone)]
pub struct ShopCatalog {
    items: Vec<Item>,
}

impl ShopCatalog {
    /// The catalog of `content/catalog/shop.json` as it was when the binary was built.
    ///
    /// # Errors
    /// [`ShopCatalogError`] with the code of the first rule it breaks.
    pub fn embedded(botarga: &BotargaCatalog) -> Result<Self, ShopCatalogError> {
        Self::parse(
            include_bytes!("../../../../content/catalog/shop.json"),
            botarga,
        )
    }

    /// Parses and checks a catalog: every id exists and has the form of an id, no item twice, and
    /// every price between 1 and [`PRICE_MAX`]. Skins are checked against their folder by a test.
    ///
    /// # Errors
    /// [`ShopCatalogError`] with the code of the first rule it breaks.
    pub fn parse(bytes: &[u8], botarga: &BotargaCatalog) -> Result<Self, ShopCatalogError> {
        let raw: RawCatalog = serde_json::from_slice(bytes).map_err(|_| invalid("json"))?;
        let mut items: Vec<Item> = Vec::with_capacity(raw.items.len());
        for item in raw.items {
            if !well_formed(&item.id) {
                return Err(invalid("id_form"));
            }
            if !(1..=PRICE_MAX).contains(&item.price) {
                return Err(invalid("price"));
            }
            let known = match item.kind {
                ShopItemKind::Accessory => botarga.accessory(&item.id).is_some(),
                ShopItemKind::Cosmetic => {
                    IdleClip::parse(&item.id).is_some() || VictoryClip::parse(&item.id).is_some()
                }
                ShopItemKind::Skin => true,
            };
            if !known {
                return Err(invalid("unknown_item"));
            }
            if items.iter().any(|i| i.kind == item.kind && i.id == item.id) {
                return Err(invalid("duplicate"));
            }
            items.push(Item {
                kind: item.kind,
                id: item.id,
                price: item.price,
            });
        }
        Ok(Self { items })
    }

    /// Every item, in the order of the file.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The item of `kind` with `id`, if the shop sells it.
    #[must_use]
    pub fn find(&self, kind: ShopItemKind, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.kind == kind && i.id == id)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn botarga() -> BotargaCatalog {
        BotargaCatalog::embedded().unwrap()
    }

    #[test]
    fn the_embedded_catalog_is_valid_and_every_skin_has_its_folder() {
        let shop = ShopCatalog::embedded(&botarga()).unwrap();
        assert!(!shop.items().is_empty());
        let skins = concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/models/skins");
        for item in shop.items() {
            if item.kind == ShopItemKind::Skin {
                let dir = std::path::Path::new(skins).join(&item.id);
                assert!(
                    dir.join(format!("{}.glb", item.id)).is_file(),
                    "{}",
                    item.id
                );
                assert!(dir.join("skin.json").is_file(), "{}", item.id);
            }
        }
    }

    #[test]
    fn a_catalog_that_breaks_a_rule_is_refused() {
        let cases = [
            (
                r#"{"items":[{"kind":"accessory","id":"nope","price":1}]}"#,
                "unknown_item",
            ),
            (
                r#"{"items":[{"kind":"cosmetic","id":"idle_9","price":1}]}"#,
                "unknown_item",
            ),
            (
                r#"{"items":[{"kind":"cosmetic","id":"idle","price":0}]}"#,
                "price",
            ),
            (
                r#"{"items":[{"kind":"cosmetic","id":"Idle","price":1}]}"#,
                "id_form",
            ),
            (
                r#"{"items":[{"kind":"cosmetic","id":"idle","price":1},{"kind":"cosmetic","id":"idle","price":2}]}"#,
                "duplicate",
            ),
            (
                r#"{"items":[{"kind":"ability","id":"x","price":1}]}"#,
                "json",
            ),
            (r#"{"items":[],"extra":1}"#, "json"),
        ];
        for (json, code) in cases {
            let error = ShopCatalog::parse(json.as_bytes(), &botarga()).unwrap_err();
            assert_eq!(error.code, code, "{json}");
        }
    }

    #[test]
    fn an_id_has_one_form() {
        assert!(well_formed("oven_mitts"));
        assert!(well_formed("win_1"));
        assert!(!well_formed(""));
        assert!(!well_formed("Oven"));
        assert!(!well_formed(&"a".repeat(65)));
        assert!(!well_formed("a/b"));
    }
}
