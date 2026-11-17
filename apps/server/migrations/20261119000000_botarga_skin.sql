-- The suit a botarga wears (feature 024, phase 2, TA-93). Additive (ADR-0006 D10): `NULL` is the
-- base botarga, as every botarga was before. The server only saves a suit the user has unlocked
-- (`botarga_unlocks` of kind `skin`); this only bounds its form, as `shop_purchases.item_id`.
ALTER TABLE botargas
    ADD COLUMN skin text NULL CONSTRAINT botargas_skin_ck CHECK (skin ~ '^[a-z0-9_-]{1,64}$');
