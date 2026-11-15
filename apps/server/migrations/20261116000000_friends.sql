-- Friends and their challenges (feature 023, phase 1, TA-86). Additive (ADR-0006 D10); every
-- constraint and trigger that can fail has a fixed name and a fixed SQLSTATE (ADR-0012 A9).
--
-- Separate from the ranked PvP of 021: no row of these tables is ever read by the matchmaking, the
-- probation, the ranking, the close of a season or `farm-report`, and no flow of friends writes
-- rating, XP, coins or `pvp_daily`.

-- The invite code of a user: 128 bits of `OsRng` in base64url (22 characters), in clear so its
-- owner can copy it again; it only makes friends and can be rotated. Born with the botarga.
CREATE TABLE friend_invites (
    user_id uuid PRIMARY KEY REFERENCES botargas,
    code text NOT NULL CONSTRAINT friend_invites_code_ck CHECK (code ~ '^[A-Za-z0-9_-]{22}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT friend_invites_code_uq UNIQUE (code)
);

-- One row per pair, without direction. Removing a friend deletes it.
CREATE TABLE friendships (
    user_lo uuid NOT NULL REFERENCES botargas,
    user_hi uuid NOT NULL REFERENCES botargas,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT friendships_pk PRIMARY KEY (user_lo, user_hi),
    CONSTRAINT friendships_order_ck CHECK (user_lo < user_hi)
);
CREATE INDEX friendships_hi_idx ON friendships (user_hi, user_lo);

-- Who removed whom (owner, 2026-11-15): `blocked_id` can no longer accept the invite of `owner_id`.
CREATE TABLE friend_blocks (
    owner_id uuid NOT NULL REFERENCES botargas,
    blocked_id uuid NOT NULL REFERENCES botargas,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT friend_blocks_pk PRIMARY KEY (owner_id, blocked_id),
    CONSTRAINT friend_blocks_self_ck CHECK (owner_id <> blocked_id)
);

-- A fight between friends: a challenge, or a match of a bracket (phase 2 adds its table and the
-- foreign key of `tournament_id`). The same canonical bytes as a player fight, so the browser
-- re-simulates it (011); private to its two sides. A row is born whole and never changes (TA014).
CREATE TABLE friend_fights (
    id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL CONSTRAINT friend_fights_kind_ck CHECK (kind IN ('challenge', 'bracket')),
    attacker_id uuid NOT NULL REFERENCES botargas,
    defender_id uuid NOT NULL REFERENCES botargas,
    client_key uuid NULL,
    ruleset_version smallint NOT NULL CONSTRAINT friend_fights_ruleset_ck
        CHECK (ruleset_version >= 2),
    spec_a bytea NOT NULL,
    spec_b bytea NOT NULL,
    seed bytea NOT NULL CONSTRAINT friend_fights_seed_ck CHECK (octet_length(seed) = 32),
    log bytea NOT NULL,
    winner text NULL CONSTRAINT friend_fights_winner_ck CHECK (winner IN ('a', 'b')),
    end_reason text NOT NULL CONSTRAINT friend_fights_end_reason_ck
        CHECK (end_reason IN ('ko', 'decision', 'draw')),
    look_a jsonb NULL CONSTRAINT friend_fights_look_a_ck
        CHECK (look_a IS NULL OR jsonb_typeof(look_a) = 'object'),
    look_b jsonb NULL CONSTRAINT friend_fights_look_b_ck
        CHECK (look_b IS NULL OR jsonb_typeof(look_b) = 'object'),
    tournament_id int NULL,
    round smallint NULL CONSTRAINT friend_fights_round_ck CHECK (round BETWEEN 1 AND 3),
    slot smallint NULL CONSTRAINT friend_fights_slot_ck CHECK (slot BETWEEN 0 AND 3),
    -- Who goes to the next round of a bracket: the winner, or after a draw the side the low bit of
    -- the first byte of the public seed says (owner, 2026-11-15), so the browser can check it.
    advances text NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT friend_fights_self_ck CHECK (attacker_id <> defender_id),
    CONSTRAINT friend_fights_result_ck CHECK ((end_reason = 'draw') = (winner IS NULL)),
    CONSTRAINT friend_fights_challenge_ck CHECK (
        (kind = 'challenge') = (client_key IS NOT NULL)
        AND (kind = 'challenge') = (tournament_id IS NULL)
        AND (tournament_id IS NULL) = (round IS NULL)
        AND (tournament_id IS NULL) = (slot IS NULL)
        AND (tournament_id IS NULL) = (advances IS NULL)
    ),
    CONSTRAINT friend_fights_advances_ck CHECK (
        advances IS NULL
        OR advances = COALESCE(winner, CASE get_byte(seed, 0) & 1 WHEN 0 THEN 'a' ELSE 'b' END)
    ),
    CONSTRAINT friend_fights_client_key_uq UNIQUE (attacker_id, client_key),
    CONSTRAINT friend_fights_bracket_uq UNIQUE (tournament_id, round, slot)
);
CREATE INDEX friend_fights_attacker_idx ON friend_fights (attacker_id, id);
CREATE INDEX friend_fights_defender_idx ON friend_fights (defender_id, id);

CREATE FUNCTION friend_fights_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA014', MESSAGE = 'friend fight guard';
    END IF;
    NEW.created_at := now();
    RETURN NEW;
END;
$$;
CREATE TRIGGER friend_fights_guard BEFORE INSERT OR UPDATE OR DELETE ON friend_fights
    FOR EACH ROW EXECUTE FUNCTION friend_fights_guard();
CREATE TRIGGER friend_fights_no_truncate BEFORE TRUNCATE ON friend_fights
    FOR EACH STATEMENT EXECUTE FUNCTION friend_fights_guard();

-- The caps of each UTC day (owner, 2026-11-15): 50 challenges and 10 tournaments per user. Only
-- the actor counts.
CREATE TABLE friend_daily (
    user_id uuid NOT NULL REFERENCES botargas,
    day date NOT NULL,
    challenges smallint NOT NULL DEFAULT 0 CONSTRAINT friend_daily_challenges_ck
        CHECK (challenges BETWEEN 0 AND 50),
    tournaments smallint NOT NULL DEFAULT 0 CONSTRAINT friend_daily_tournaments_ck
        CHECK (tournaments BETWEEN 0 AND 10),
    CONSTRAINT friend_daily_pk PRIMARY KEY (user_id, day)
);

-- One event per challenge, tournament and acceptance, always of the actor, with the `ip_hash` of
-- the request (retention of D8). Never a `pvp_attack`.
ALTER TABLE auth_events
    ADD COLUMN friend_fight_id int NULL REFERENCES friend_fights,
    ADD COLUMN friend_tournament_id int NULL,
    DROP CONSTRAINT auth_events_kind_ck,
    ADD CONSTRAINT auth_events_kind_ck
        CHECK (kind IN ('login', 'link', 'unlink', 'grant', 'reauth', 'bailout', 'bet', 'pvp_attack',
                        'friend_challenge', 'friend_tournament', 'friend_accept')),
    ADD CONSTRAINT auth_events_friend_fight_ck
        CHECK ((kind = 'friend_challenge') = (friend_fight_id IS NOT NULL)),
    ADD CONSTRAINT auth_events_friend_tournament_ck
        CHECK ((kind = 'friend_tournament') = (friend_tournament_id IS NOT NULL));
