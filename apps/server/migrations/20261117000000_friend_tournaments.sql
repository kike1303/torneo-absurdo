-- Tournaments between friends (feature 023, phase 2, TA-87). Additive (ADR-0006 D10); every
-- constraint and trigger that can fail has a fixed name and a fixed SQLSTATE (ADR-0012 A9).
--
-- A tournament is resolved whole in the transaction that creates it: its row, its slots and its
-- matches (`friend_fights` with `kind = 'bracket'`) are born together and never change (TA014).

-- A knockout of 4 or 8 botargas (owner, 2026-11-15). The owner holds one slot.
CREATE TABLE friend_tournaments (
    id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    owner_id uuid NOT NULL REFERENCES botargas,
    size smallint NOT NULL CONSTRAINT friend_tournaments_size_ck CHECK (size IN (4, 8)),
    client_key uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT friend_tournaments_client_key_uq UNIQUE (owner_id, client_key)
);

-- The order of the bracket, drawn with `OsRng`: slot `2k` meets slot `2k + 1` in round 1.
CREATE TABLE friend_tournament_slots (
    tournament_id int NOT NULL REFERENCES friend_tournaments,
    slot smallint NOT NULL CONSTRAINT friend_tournament_slots_slot_ck CHECK (slot BETWEEN 0 AND 7),
    user_id uuid NOT NULL REFERENCES botargas,
    CONSTRAINT friend_tournament_slots_pk PRIMARY KEY (tournament_id, slot),
    CONSTRAINT friend_tournament_slots_user_uq UNIQUE (tournament_id, user_id)
);
CREATE INDEX friend_tournament_slots_user_idx ON friend_tournament_slots (user_id, tournament_id);

-- The same guard as `friend_fights`: insert only.
CREATE TRIGGER friend_tournaments_guard BEFORE INSERT OR UPDATE OR DELETE ON friend_tournaments
    FOR EACH ROW EXECUTE FUNCTION friend_fights_guard();
CREATE TRIGGER friend_tournaments_no_truncate BEFORE TRUNCATE ON friend_tournaments
    FOR EACH STATEMENT EXECUTE FUNCTION friend_fights_guard();

CREATE FUNCTION friend_tournament_slots_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA014', MESSAGE = 'friend fight guard';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER friend_tournament_slots_guard
    BEFORE INSERT OR UPDATE OR DELETE ON friend_tournament_slots
    FOR EACH ROW EXECUTE FUNCTION friend_tournament_slots_guard();
CREATE TRIGGER friend_tournament_slots_no_truncate BEFORE TRUNCATE ON friend_tournament_slots
    FOR EACH STATEMENT EXECUTE FUNCTION friend_tournament_slots_guard();

-- The references phase 1 left open.
ALTER TABLE friend_fights
    ADD CONSTRAINT friend_fights_tournament_fk
        FOREIGN KEY (tournament_id) REFERENCES friend_tournaments;
ALTER TABLE auth_events
    ADD CONSTRAINT auth_events_friend_tournament_fk
        FOREIGN KEY (friend_tournament_id) REFERENCES friend_tournaments;
