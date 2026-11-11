-- The player fights (feature 021, phase 2, TA-63): one row per attack, resolved by the server in
-- the transaction that writes it, and the counter of the attacks of each UTC day. Additive
-- (ADR-0006 D10). As in the earlier migrations, every constraint that can fail has a fixed name
-- (ADR-0012 A9).
--
-- A row is born whole and never changes: there is no window and no state machine, so the guard
-- refuses any UPDATE or DELETE (TA011). The seed is stored in clear from the start: nobody bets on
-- a player fight (ADR-0002 is of the tournament). `spec_a`, `spec_b` and `log` are the canonical
-- JSON of the engine, as in `fights`, so the browser re-simulates the same bytes (011).
--
-- `rating_delta`, `xp_attacker` and `xp_defender` are written by phases 3 and 4 (TA-62, TA-64);
-- until then the server writes 0. Their ranges are already the ones of the numbers of 021: the
-- Elo moves at most K = 40, and only a fight against a user moves it or pays.
CREATE TABLE pvp_fights (
    id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    attacker_id uuid NOT NULL REFERENCES botargas,
    -- NULL only against a character of the tournament, named by `defender_slug`.
    defender_id uuid NULL REFERENCES botargas,
    defender_slug text NULL CONSTRAINT pvp_fights_defender_slug_ck
        CHECK (defender_slug ~ '^[a-z][a-z0-9-]{1,31}$'),
    client_key uuid NOT NULL,
    ruleset_version smallint NOT NULL CONSTRAINT pvp_fights_ruleset_ck CHECK (ruleset_version >= 2),
    spec_a bytea NOT NULL,
    spec_b bytea NOT NULL,
    seed bytea NOT NULL CONSTRAINT pvp_fights_seed_ck CHECK (octet_length(seed) = 32),
    log bytea NOT NULL,
    winner text NULL CONSTRAINT pvp_fights_winner_ck CHECK (winner IN ('a', 'b')),
    end_reason text NOT NULL CONSTRAINT pvp_fights_end_reason_ck
        CHECK (end_reason IN ('ko', 'decision', 'draw')),
    rating_delta smallint NOT NULL CONSTRAINT pvp_fights_rating_delta_ck
        CHECK (rating_delta BETWEEN -40 AND 40),
    xp_attacker int NOT NULL CONSTRAINT pvp_fights_xp_attacker_ck CHECK (xp_attacker >= 0),
    xp_defender int NOT NULL CONSTRAINT pvp_fights_xp_defender_ck CHECK (xp_defender >= 0),
    coins_attacker int NOT NULL CONSTRAINT pvp_fights_coins_ck CHECK (coins_attacker >= 0),
    daily_index smallint NOT NULL CONSTRAINT pvp_fights_daily_index_ck CHECK (daily_index >= 1),
    -- Both set by `pvp_fights_guard`, whatever the application sends: `YYYYMM` of the UTC month.
    season_id int NOT NULL,
    created_at timestamptz NOT NULL,
    CONSTRAINT pvp_fights_defender_ck CHECK ((defender_id IS NULL) <> (defender_slug IS NULL)),
    CONSTRAINT pvp_fights_self_ck CHECK (defender_id <> attacker_id),
    -- A draw has no winner, and a KO or a decision has one.
    CONSTRAINT pvp_fights_result_ck CHECK ((end_reason = 'draw') = (winner IS NULL)),
    -- 021 "Sin rival": the character of the tournament never moves the rating, pays no coins and
    -- has no XP of its own.
    CONSTRAINT pvp_fights_tournament_ck
        CHECK (defender_id IS NOT NULL OR (rating_delta = 0 AND coins_attacker = 0 AND xp_defender = 0)),
    -- 021 "Buscar pelea" row 2: a retry with the same key is the same fight.
    CONSTRAINT pvp_fights_client_key_uq UNIQUE (attacker_id, client_key),
    -- The target of `ledger_entries_pvp_owner_fk`: a reward pays the attacker of its fight.
    CONSTRAINT pvp_fights_attacker_uq UNIQUE (id, attacker_id)
);
CREATE INDEX pvp_fights_attacker_idx ON pvp_fights (attacker_id, id);
CREATE INDEX pvp_fights_defender_idx ON pvp_fights (defender_id, id) WHERE defender_id IS NOT NULL;

CREATE FUNCTION pvp_fights_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA011', MESSAGE = 'pvp fight guard';
    END IF;
    NEW.created_at := now();
    NEW.season_id := (extract(year FROM now() AT TIME ZONE 'UTC') * 100
                      + extract(month FROM now() AT TIME ZONE 'UTC'))::int;
    RETURN NEW;
END;
$$;
CREATE TRIGGER pvp_fights_guard BEFORE INSERT OR UPDATE OR DELETE ON pvp_fights
    FOR EACH ROW EXECUTE FUNCTION pvp_fights_guard();
CREATE TRIGGER pvp_fights_no_truncate BEFORE TRUNCATE ON pvp_fights
    FOR EACH STATEMENT EXECUTE FUNCTION pvp_fights_guard();

-- 021 "Emparejamiento": a user with a session in the last 30 days is preferred as a defender, read
-- from `users` and never by joining `sessions`. NULL until the user's next request: the touch of
-- the session refreshes it at most once a day, and never waits for the user's lock.
ALTER TABLE users ADD COLUMN last_seen_at timestamptz NULL;

-- 021 "Escalones del día": the attacks of each user in each UTC day, in the database and not in
-- memory. Only the attacker counts: a defense never writes here.
CREATE TABLE pvp_daily (
    user_id uuid NOT NULL REFERENCES botargas,
    day date NOT NULL,
    attacks smallint NOT NULL CONSTRAINT pvp_daily_attacks_ck CHECK (attacks >= 1),
    CONSTRAINT pvp_daily_pk PRIMARY KEY (user_id, day)
);

-- 021 step 5: the coins of an attack, only as an entry, from the issuance to its attacker, once per
-- fight whatever the application does.
ALTER TABLE ledger_entries
    ADD COLUMN pvp_fight_id int NULL,
    DROP CONSTRAINT ledger_entries_kind_ck,
    ADD CONSTRAINT ledger_entries_kind_ck
        CHECK (kind IN ('grant', 'bailout', 'bet_stake', 'bet_payout', 'refund', 'rake', 'pvp_reward')),
    ADD CONSTRAINT ledger_entries_pvp_ck CHECK ((kind = 'pvp_reward') = (pvp_fight_id IS NOT NULL)),
    ADD CONSTRAINT ledger_entries_pvp_owner_fk FOREIGN KEY (pvp_fight_id, user_id)
        REFERENCES pvp_fights (id, attacker_id);
CREATE UNIQUE INDEX ledger_entries_pvp_reward_uq ON ledger_entries (pvp_fight_id)
    WHERE kind = 'pvp_reward';

-- The body of 20261103000000_bet_stake_amount.sql with two changes: the issuance also pays a
-- `pvp_reward`, and a `pvp_reward` moves exactly the `coins_attacker` of its fight, from the
-- issuance to the attacker (the owner is the foreign key above), and nothing else.
CREATE OR REPLACE FUNCTION ledger_posting_rules() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    account_kind text;
    account_user uuid;
    entry_kind text;
    entry_user uuid;
    entry_bet uuid;
    entry_pvp int;
    bet_stake bigint;
    pvp_coins bigint;
BEGIN
    SELECT kind, user_id INTO account_kind, account_user
        FROM ledger_accounts WHERE id = NEW.account_id;
    SELECT kind, user_id, bet_id, pvp_fight_id INTO entry_kind, entry_user, entry_bet, entry_pvp
        FROM ledger_entries WHERE id = NEW.entry_id;
    IF (account_kind = 'rake_sink' AND NEW.amount < 0)
       OR (account_kind = 'issuance' AND NEW.amount < 0
           AND entry_kind NOT IN ('grant', 'bailout', 'pvp_reward'))
       OR (account_kind = 'bet_escrow' AND NEW.amount > 0 AND entry_kind <> 'bet_stake')
       OR (account_kind = 'bet_escrow' AND NEW.amount < 0
           AND entry_kind NOT IN ('bet_payout', 'refund', 'rake'))
       OR (account_kind = 'user' AND account_user IS DISTINCT FROM entry_user)
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
    END IF;
    IF entry_kind = 'bet_stake' THEN
        SELECT stake INTO bet_stake FROM bets WHERE id = entry_bet;
        IF bet_stake IS NULL
           OR NOT ((account_kind = 'bet_escrow' AND NEW.amount = bet_stake)
                   OR (account_kind = 'user' AND NEW.amount = -bet_stake))
        THEN
            RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
        END IF;
    END IF;
    IF entry_kind = 'pvp_reward' THEN
        SELECT coins_attacker INTO pvp_coins FROM pvp_fights WHERE id = entry_pvp;
        IF pvp_coins IS NULL
           OR NOT ((account_kind = 'issuance' AND NEW.amount = -pvp_coins)
                   OR (account_kind = 'user' AND NEW.amount = pvp_coins))
        THEN
            RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

-- 021 step 5: one event per attack, with the `ip_hash` of the request (retention of D8).
ALTER TABLE auth_events
    ADD COLUMN pvp_fight_id int NULL REFERENCES pvp_fights,
    DROP CONSTRAINT auth_events_kind_ck,
    ADD CONSTRAINT auth_events_kind_ck
        CHECK (kind IN ('login', 'link', 'unlink', 'grant', 'reauth', 'bailout', 'bet', 'pvp_attack')),
    ADD CONSTRAINT auth_events_pvp_ck CHECK ((kind = 'pvp_attack') = (pvp_fight_id IS NOT NULL));
