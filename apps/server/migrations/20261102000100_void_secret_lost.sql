-- The fight voided with its secret lost (feature 012, phase A: RF19, RF20 and the index of RF23). It
-- implements the amendment of ADR-0002 and is on its own: without that amendment, this file goes.
--
-- As in the earlier migrations, every constraint and trigger that can fail has a fixed name, and
-- every RAISE has a fixed SQLSTATE and message and no values (ADR-0012 A9).

ALTER TABLE fights
    DROP CONSTRAINT fights_void_reason_ck,
    ADD CONSTRAINT fights_void_reason_ck
        CHECK (void_reason IN ('snapshot_invalid', 'engine_error', 'secret_lost')),
    DROP CONSTRAINT fights_reveal_ck,
    -- The only form without a seed. `IS NOT DISTINCT FROM`: with `=`, a final without a void reason
    -- would make this NULL, and a CHECK lets a NULL through.
    ADD CONSTRAINT fights_reveal_ck CHECK (
        state <> 'settled' OR seed_commit IS NULL
        OR (void_reason IS NOT DISTINCT FROM 'secret_lost' AND revealed_seed IS NULL)
        OR (revealed_seed IS NOT NULL AND sha256(revealed_seed) = seed_commit)
    );

-- RF19: until this hour an emission may still be running. 5000 is `SETTLE_DELAY_MS` and 120000 is
-- `SECRET_LOST_MARGIN_MS`: a test ties them. In `betting_closed`, without `live_started_at`, `live_at`
-- rules: going live would change the row that the command holds locked.
CREATE FUNCTION fight_secret_lost_deadline(started timestamptz, live_at timestamptz, max_ms int)
RETURNS timestamptz LANGUAGE sql STABLE AS $$
    SELECT COALESCE(started, live_at) + (max_ms + 5000 + 120000) * interval '1 millisecond'
$$;

-- RF23: the public counter of the voided fights.
CREATE INDEX fights_voided_idx ON fights (void_reason) WHERE void_reason IS NOT NULL;

-- RF20: only the two lines of the reveal are conditioned. Everything else of this edge, and the block
-- on `end_reason`, `winner` and `log` that follows it, rules `secret_lost` too.
CREATE OR REPLACE FUNCTION fights_guard() RETURNS trigger LANGUAGE plpgsql AS $$
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
           NEW.time_limit_ms, NEW.max_duration_ms, NEW.matchup_round, NEW.roster_sha256,
           NEW.rake_bp)
       IS DISTINCT FROM
       ROW(OLD.id, OLD.opens_at, OLD.closes_at, OLD.live_at, OLD.ruleset_version, OLD.spec_a,
           OLD.spec_b, OLD.snapshot_sha256, OLD.slug_a, OLD.slug_b, OLD.max_hp_a, OLD.max_hp_b,
           OLD.time_limit_ms, OLD.max_duration_ms, OLD.matchup_round, OLD.roster_sha256,
           OLD.rake_bp)
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
               -- The seed is always revealed (010 RF9), but for the only final without one: the row
               -- of its secret is gone, and no emission can be running (012 RF19, RF20).
               -- In parentheses: PL/pgSQL ends the condition of an IF at the first THEN outside them.
               OR (CASE WHEN NEW.void_reason IS NOT DISTINCT FROM 'secret_lost' THEN
                       NEW.revealed_seed IS NOT NULL
                       OR EXISTS (SELECT 1 FROM fight_secrets WHERE fight_id = NEW.id)
                       OR now() < fight_secret_lost_deadline(OLD.live_started_at, NEW.live_at,
                                                             NEW.max_duration_ms)
                   ELSE
                       NEW.revealed_seed IS NULL
                       OR sha256(NEW.revealed_seed) IS DISTINCT FROM NEW.seed_commit
                   END)
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
