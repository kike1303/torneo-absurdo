-- What each botarga wore in a player fight (feature 021, TA-67 and TA-84): its idle, its victory
-- and its accessories, as `PvpLook` of the protocol. Additive (ADR-0006 D10); every constraint that
-- can fail has a fixed name (ADR-0012 A9).
--
-- Outside what the fight hashes: the result does not depend on them and the browser does not verify
-- them, it only draws them. NULL for a character of the tournament and for every fight of before
-- this migration. The row stays immutable (`pvp_fights_guard`): they are written with it.
ALTER TABLE pvp_fights
    ADD COLUMN look_a jsonb NULL,
    ADD COLUMN look_b jsonb NULL,
    ADD CONSTRAINT pvp_fights_look_a_ck CHECK (look_a IS NULL OR jsonb_typeof(look_a) = 'object'),
    ADD CONSTRAINT pvp_fights_look_b_ck CHECK (
        look_b IS NULL OR (jsonb_typeof(look_b) = 'object' AND defender_id IS NOT NULL)
    );
