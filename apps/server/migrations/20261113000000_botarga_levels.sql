-- Levels, XP and the draws of new attacks (feature 021, phase 3, TA-62; 016 phase 5b). Additive
-- (ADR-0006 D10); every constraint that can fail has a fixed name (ADR-0012 A9).
--
-- `botargas.xp` is the XP inside the current level: the server subtracts `xp_to_next` each time it
-- crosses one, in the transaction of the fight. At the last level there is nothing left to earn, so
-- it stays 0. Every row of before this migration has `xp = 0`, which both constraints accept.
ALTER TABLE botargas
    ADD CONSTRAINT botargas_xp_level_ck CHECK (xp < 100 + 20 * level AND (level < 100 OR xp = 0));

-- A draw (016 "Sorteo de ataques"): what a level that is a multiple of 5 offers, written in the same
-- transaction that crosses it. The options are drawn by the server with `OsRng` among what the user
-- does not own (never the PRNG of the engine), and stay here for good: a reload or a retry reads the
-- same ones, and the row is the record of the draw for an audit. One per level and user, so a level
-- never draws twice. Several can be pending at once (two levels in a row without choosing); the
-- server never offers in a new draw an option that a pending one already offers.
--
-- `kind = 'cosmetic'`: no attack of the tiers of the level was left, so the draw offers clip codes.
CREATE TABLE botarga_draws (
    id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES botargas,
    level smallint NOT NULL CONSTRAINT botarga_draws_level_ck
        CHECK (level BETWEEN 5 AND 100 AND level % 5 = 0),
    kind text NOT NULL CONSTRAINT botarga_draws_kind_ck CHECK (kind IN ('ability', 'cosmetic')),
    options text[] NOT NULL CONSTRAINT botarga_draws_options_ck CHECK (
        cardinality(options) BETWEEN 1 AND 3 AND array_position(options, NULL) IS NULL
    ),
    chosen text NULL CONSTRAINT botarga_draws_chosen_ck CHECK (chosen = ANY (options)),
    created_at timestamptz NOT NULL DEFAULT now(),
    chosen_at timestamptz NULL,
    CONSTRAINT botarga_draws_chosen_at_ck CHECK ((chosen IS NULL) = (chosen_at IS NULL)),
    CONSTRAINT botarga_draws_level_uq UNIQUE (user_id, level)
);

CREATE INDEX botarga_draws_pending_ix ON botarga_draws (user_id, id) WHERE chosen IS NULL;

-- A draw is born with its options and is chosen once: the only change is `chosen` (and its
-- `chosen_at`) from NULL to one of the options. Never deleted (TA012).
CREATE FUNCTION botarga_draws_guard() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE'
        AND OLD.chosen IS NULL
        AND NEW.chosen IS NOT NULL
        AND NEW.id = OLD.id
        AND NEW.user_id = OLD.user_id
        AND NEW.level = OLD.level
        AND NEW.kind = OLD.kind
        AND NEW.options = OLD.options
        AND NEW.created_at = OLD.created_at
    THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION USING ERRCODE = 'TA012', MESSAGE = 'botarga draw guard';
END;
$$;
CREATE TRIGGER botarga_draws_guard BEFORE UPDATE OR DELETE ON botarga_draws
    FOR EACH ROW EXECUTE FUNCTION botarga_draws_guard();
CREATE TRIGGER botarga_draws_no_truncate BEFORE TRUNCATE ON botarga_draws
    FOR EACH STATEMENT EXECUTE FUNCTION botarga_draws_guard();
