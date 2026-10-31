-- The scheduled fights (feature 010, phase A: RF1, RF4, RF8, RF9, RF25, RF59 and the amendment of
-- phase A, DP-3 and DP-6).
--
-- As in the earlier migrations, every constraint and trigger that can fail has a fixed name, and
-- every RAISE has a fixed SQLSTATE and message and no values (ADR-0012 A9). `bytea`, never `jsonb`:
-- `jsonb` reorders keys and would break "byte by byte". The guards run before the CHECKs, so the
-- rules a CHECK repeats answer TA006; the CHECKs are the second barrier (23514).

CREATE TABLE fights (
    id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    state text NOT NULL CONSTRAINT fights_state_ck
        CHECK (state IN ('scheduled', 'betting_open', 'betting_closed', 'live', 'settled')),
    opens_at timestamptz NOT NULL,
    closes_at timestamptz NOT NULL,
    live_at timestamptz NOT NULL,
    ruleset_version smallint NOT NULL CONSTRAINT fights_ruleset_version_ck
        CHECK (ruleset_version > 0),
    -- The bytes of canonical_spec_json, and the hash of RF2.
    spec_a bytea NOT NULL,
    spec_b bytea NOT NULL,
    snapshot_sha256 bytea NOT NULL CONSTRAINT fights_snapshot_sha256_ck
        CHECK (octet_length(snapshot_sha256) = 32),
    -- Derived for the summary. They never feed the engine.
    slug_a text NOT NULL,
    slug_b text NOT NULL,
    max_hp_a int NOT NULL CONSTRAINT fights_max_hp_a_ck CHECK (max_hp_a BETWEEN 1 AND 65535),
    max_hp_b int NOT NULL CONSTRAINT fights_max_hp_b_ck CHECK (max_hp_b BETWEEN 1 AND 65535),
    time_limit_ms int NOT NULL CONSTRAINT fights_time_limit_ms_ck CHECK (time_limit_ms > 0),
    max_duration_ms int NOT NULL CONSTRAINT fights_max_duration_ms_ck
        CHECK (max_duration_ms >= time_limit_ms),
    -- RF20: the round of the pair, and the SHA-256 of the sorted slugs of the roster.
    matchup_round int NOT NULL CONSTRAINT fights_matchup_round_ck CHECK (matchup_round > 0),
    roster_sha256 bytea NOT NULL CONSTRAINT fights_roster_sha256_ck
        CHECK (octet_length(roster_sha256) = 32),
    seed_commit bytea NULL CONSTRAINT fights_seed_commit_ck
        CHECK (seed_commit IS NULL OR octet_length(seed_commit) = 32),
    opened_at timestamptz NULL,
    closed_at timestamptz NULL,
    live_started_at timestamptz NULL,
    settled_at timestamptz NULL,
    winner text NULL CONSTRAINT fights_winner_value_ck CHECK (winner IN ('a', 'b')),
    end_reason text NULL CONSTRAINT fights_end_reason_ck
        CHECK (end_reason IN ('ko', 'decision', 'draw')),
    cancel_reason text NULL CONSTRAINT fights_cancel_reason_ck
        CHECK (cancel_reason IN ('missed_window', 'fight_unrunnable')),
    void_reason text NULL CONSTRAINT fights_void_reason_ck
        CHECK (void_reason IN ('snapshot_invalid', 'engine_error')),
    revealed_seed bytea NULL CONSTRAINT fights_revealed_seed_ck
        CHECK (revealed_seed IS NULL OR (octet_length(revealed_seed) = 32 AND state = 'settled')),
    log bytea NULL CONSTRAINT fights_log_ck CHECK (log IS NULL OR state = 'settled'),
    CONSTRAINT fights_opens_at_key UNIQUE (opens_at),
    CONSTRAINT fights_hours_ck CHECK (closes_at > opens_at AND live_at >= closes_at),
    -- RF9: the commit exists from betting_open on, except on a cancelled fight.
    CONSTRAINT fights_commit_ck
        CHECK (state = 'scheduled' OR seed_commit IS NOT NULL OR cancel_reason IS NOT NULL),
    CONSTRAINT fights_final_ck CHECK (
        num_nonnulls(end_reason, cancel_reason, void_reason) = CASE WHEN state = 'settled' THEN 1 ELSE 0 END
    ),
    CONSTRAINT fights_settled_at_ck CHECK ((state = 'settled') = (settled_at IS NOT NULL)),
    CONSTRAINT fights_winner_ck CHECK (winner IS NULL OR end_reason IS NOT NULL),
    CONSTRAINT fights_draw_ck CHECK (end_reason IS NULL OR (winner IS NULL) = (end_reason = 'draw')),
    CONSTRAINT fights_decided_log_ck CHECK (end_reason IS NULL OR log IS NOT NULL),
    CONSTRAINT fights_cancelled_ck CHECK (
        cancel_reason IS NULL OR (seed_commit IS NULL AND revealed_seed IS NULL AND log IS NULL)
    ),
    CONSTRAINT fights_voided_ck CHECK (void_reason IS NULL OR log IS NULL),
    CONSTRAINT fights_reveal_ck CHECK (
        state <> 'settled' OR seed_commit IS NULL
        OR (revealed_seed IS NOT NULL AND sha256(revealed_seed) = seed_commit)
    )
);
CREATE INDEX fights_unsettled_idx ON fights (opens_at) WHERE state <> 'settled';

-- The seed lives only here, so a SELECT on `fights` can never bring it (RF25).
CREATE TABLE fight_secrets (
    fight_id int PRIMARY KEY REFERENCES fights,
    seed bytea NOT NULL CONSTRAINT fight_secrets_seed_ck CHECK (octet_length(seed) = 32)
);

CREATE FUNCTION fights_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    transition text;
BEGIN
    IF TG_OP = 'INSERT' THEN
        -- DP-6: a row is born scheduled, with its whole cycle ahead.
        IF NEW.state <> 'scheduled' OR num_nonnulls(
            NEW.seed_commit, NEW.opened_at, NEW.closed_at, NEW.live_started_at, NEW.settled_at,
            NEW.winner, NEW.end_reason, NEW.cancel_reason, NEW.void_reason, NEW.revealed_seed, NEW.log
        ) <> 0 THEN
            RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
        END IF;
        RETURN NEW;
    END IF;

    -- RF9: a settled row admits no UPDATE at all.
    IF OLD.state = 'settled' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
    END IF;

    -- RF4: what was fixed when the fight was announced.
    IF ROW(NEW.id, NEW.opens_at, NEW.closes_at, NEW.live_at, NEW.ruleset_version, NEW.spec_a,
           NEW.spec_b, NEW.snapshot_sha256, NEW.slug_a, NEW.slug_b, NEW.max_hp_a, NEW.max_hp_b,
           NEW.time_limit_ms, NEW.max_duration_ms, NEW.matchup_round, NEW.roster_sha256)
       IS DISTINCT FROM
       ROW(OLD.id, OLD.opens_at, OLD.closes_at, OLD.live_at, OLD.ruleset_version, OLD.spec_a,
           OLD.spec_b, OLD.snapshot_sha256, OLD.slug_a, OLD.slug_b, OLD.max_hp_a, OLD.max_hp_b,
           OLD.time_limit_ms, OLD.max_duration_ms, OLD.matchup_round, OLD.roster_sha256)
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
    END IF;

    -- RF4: a value already written is never rewritten.
    IF (OLD.seed_commit IS NOT NULL AND NEW.seed_commit IS DISTINCT FROM OLD.seed_commit)
       OR (OLD.opened_at IS NOT NULL AND NEW.opened_at IS DISTINCT FROM OLD.opened_at)
       OR (OLD.closed_at IS NOT NULL AND NEW.closed_at IS DISTINCT FROM OLD.closed_at)
       OR (OLD.live_started_at IS NOT NULL AND NEW.live_started_at IS DISTINCT FROM OLD.live_started_at)
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
    END IF;

    -- RF8 and DP-6: every legitimate UPDATE is one edge of the graph, and writes only its columns.
    transition := OLD.state || '>' || NEW.state;
    CASE transition
        WHEN 'scheduled>betting_open' THEN
            IF NEW.seed_commit IS NULL OR NEW.opened_at IS NULL
               OR num_nonnulls(NEW.closed_at, NEW.live_started_at, NEW.settled_at, NEW.winner,
                               NEW.end_reason, NEW.cancel_reason, NEW.void_reason,
                               NEW.revealed_seed, NEW.log) <> 0
               -- Security review R2: no commit is published without the seed it commits to.
               OR NOT EXISTS (SELECT 1 FROM fight_secrets
                              WHERE fight_id = NEW.id AND sha256(seed) = NEW.seed_commit)
            THEN
                RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
            END IF;
        WHEN 'betting_open>betting_closed' THEN
            IF NEW.closed_at IS NULL
               OR num_nonnulls(NEW.live_started_at, NEW.settled_at, NEW.winner, NEW.end_reason,
                               NEW.cancel_reason, NEW.void_reason, NEW.revealed_seed, NEW.log) <> 0
            THEN
                RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
            END IF;
        WHEN 'betting_closed>live' THEN
            IF NEW.live_started_at IS NULL
               OR num_nonnulls(NEW.settled_at, NEW.winner, NEW.end_reason, NEW.cancel_reason,
                               NEW.void_reason, NEW.revealed_seed, NEW.log) <> 0
            THEN
                RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
            END IF;
        WHEN 'scheduled>settled' THEN
            -- Cancelled: no seed ever existed.
            IF NEW.settled_at IS NULL OR NEW.cancel_reason IS NULL
               OR num_nonnulls(NEW.seed_commit, NEW.opened_at, NEW.closed_at, NEW.live_started_at,
                               NEW.winner, NEW.end_reason, NEW.void_reason, NEW.revealed_seed,
                               NEW.log) <> 0
            THEN
                RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
            END IF;
        WHEN 'live>settled', 'betting_closed>settled' THEN
            IF NEW.settled_at IS NULL
               OR NEW.cancel_reason IS NOT NULL
               OR NEW.live_started_at IS DISTINCT FROM OLD.live_started_at
               -- Without exceptions, voided too: the seed is always revealed (RF9).
               OR NEW.revealed_seed IS NULL
               OR sha256(NEW.revealed_seed) IS DISTINCT FROM NEW.seed_commit
            THEN
                RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
            END IF;
            IF NEW.void_reason IS NOT NULL THEN
                IF num_nonnulls(NEW.end_reason, NEW.winner, NEW.log) <> 0 THEN
                    RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
                END IF;
            ELSIF transition <> 'live>settled'
                  OR NEW.end_reason IS NULL OR NEW.log IS NULL
                  OR (NEW.winner IS NULL) IS DISTINCT FROM (NEW.end_reason = 'draw')
            THEN
                RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
            END IF;
        ELSE
            RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
    END CASE;
    RETURN NEW;
END;
$$;
CREATE TRIGGER fights_guard BEFORE INSERT OR UPDATE ON fights
    FOR EACH ROW EXECUTE FUNCTION fights_guard();

CREATE FUNCTION fights_refuse() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
END;
$$;
CREATE TRIGGER fights_no_delete BEFORE DELETE ON fights
    FOR EACH ROW EXECUTE FUNCTION fights_refuse();
CREATE TRIGGER fights_no_truncate BEFORE TRUNCATE ON fights
    FOR EACH STATEMENT EXECUTE FUNCTION fights_refuse();

CREATE FUNCTION fight_secrets_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    fight_state text;
    commit_value bytea;
BEGIN
    IF TG_OP = 'UPDATE' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
    END IF;
    IF TG_OP = 'INSERT' THEN
        SELECT state, seed_commit INTO fight_state, commit_value FROM fights WHERE id = NEW.fight_id;
        -- DP-6: nothing to restore on a settled fight. R2: a restored secret matches its commit.
        -- A missing fight is the foreign key's error.
        IF fight_state = 'settled'
           OR (commit_value IS NOT NULL AND sha256(NEW.seed) <> commit_value)
        THEN
            RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
        END IF;
        RETURN NEW;
    END IF;
    -- A DELETE, only of a fight this transaction sees settled (RF25, amendment DP-6).
    SELECT state INTO fight_state FROM fights WHERE id = OLD.fight_id;
    IF fight_state IS DISTINCT FROM 'settled' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
    END IF;
    RETURN OLD;
END;
$$;
CREATE TRIGGER fight_secrets_guard BEFORE INSERT OR UPDATE OR DELETE ON fight_secrets
    FOR EACH ROW EXECUTE FUNCTION fight_secrets_guard();
CREATE TRIGGER fight_secrets_no_truncate BEFORE TRUNCATE ON fight_secrets
    FOR EACH STATEMENT EXECUTE FUNCTION fights_refuse();
