-- Seasons and their leaderboard (feature 020, phase A; ADR-0008 D8). Additive (ADR-0006 D10).
--
-- A season is a calendar month in UTC. The season of a fight is the month of its `closes_at`, the
-- same formula everywhere and never the session's `TimeZone`. Every constraint and trigger that can
-- fail has a fixed name and a fixed SQLSTATE with no values (ADR-0012 A9).
--
-- Nothing is summed here: `seasons::reconcile`, at startup and in the daily job, sums every settled
-- fight without a mark in `season_tallies`, which covers what was settled before this migration and
-- what an old binary settles during a deploy.
--
-- The retention of `auth_events` (and so of `ip_hash`) goes from 30 to 38 days with this feature:
-- `max(30 days, season + 7 days)` of ADR-0008 D8, with a season of up to 31 days. It is code of
-- `jobs.rs`; the comment "Kept 30 days" of 20261028000000_auth.sql is history, and stays as it is.

-- `id` is YYYYMM; the CHECKs keep it, `starts_at` and `ends_at` the same month in UTC.
CREATE TABLE seasons (
    id int PRIMARY KEY,
    starts_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL,
    closed_at timestamptz NULL,
    CONSTRAINT seasons_month_ck CHECK (
        starts_at = date_trunc('month', starts_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
        AND ends_at = (date_trunc('month', starts_at AT TIME ZONE 'UTC') + interval '1 month')
            AT TIME ZONE 'UTC'
        AND id = extract(year FROM starts_at AT TIME ZONE 'UTC')::int * 100
            + extract(month FROM starts_at AT TIME ZONE 'UTC')::int
    )
);

-- The RankedProfit of a user in a season (ADR-0008 D8): only `seasons::tally` adds to it, and only
-- the CLI writes `excluded_*` (phase B).
CREATE TABLE season_stats (
    season_id int NOT NULL REFERENCES seasons,
    user_id uuid NOT NULL REFERENCES users,
    ranked_profit bigint NOT NULL DEFAULT 0,
    settled_fights int NOT NULL DEFAULT 0 CONSTRAINT season_stats_fights_ck CHECK (settled_fights >= 0),
    excluded_at timestamptz NULL,
    excluded_by uuid NULL REFERENCES users,
    excluded_reason text NULL,
    PRIMARY KEY (season_id, user_id),
    CONSTRAINT season_stats_excluded_ck
        CHECK (num_nonnulls(excluded_at, excluded_by, excluded_reason) IN (0, 3))
);
CREATE INDEX season_stats_board_idx
    ON season_stats (season_id, ranked_profit DESC, settled_fights DESC, user_id);

-- One row per fight already summed: what makes the hook, the reconciliation and a retry never sum
-- twice (020, security review 7).
CREATE TABLE season_tallies (
    fight_id int PRIMARY KEY REFERENCES fights,
    season_id int NOT NULL REFERENCES seasons,
    tallied_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX season_tallies_season_idx ON season_tallies (season_id);
CREATE TRIGGER season_tallies_immutable BEFORE UPDATE OR DELETE ON season_tallies
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER season_tallies_no_truncate BEFORE TRUNCATE ON season_tallies
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();

-- A season keeps its month, and `closed_at` is written once, only by the CLI (`torneo.actor`).
CREATE FUNCTION seasons_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA009', MESSAGE = 'season guard';
    END IF;
    IF TG_OP = 'INSERT' THEN
        IF NEW.closed_at IS NOT NULL THEN
            RAISE EXCEPTION USING ERRCODE = 'TA009', MESSAGE = 'season guard';
        END IF;
        RETURN NEW;
    END IF;
    IF ROW(NEW.id, NEW.starts_at, NEW.ends_at) IS DISTINCT FROM ROW(OLD.id, OLD.starts_at, OLD.ends_at)
       OR OLD.closed_at IS NOT NULL
       OR (NEW.closed_at IS DISTINCT FROM OLD.closed_at
           AND current_setting('torneo.actor', true) IS DISTINCT FROM 'cli')
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA009', MESSAGE = 'season guard';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER seasons_guard BEFORE INSERT OR UPDATE OR DELETE ON seasons
    FOR EACH ROW EXECUTE FUNCTION seasons_guard();
CREATE TRIGGER seasons_no_truncate BEFORE TRUNCATE ON seasons
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();

-- `excluded_*` only from the CLI; the upsert of `tally`, which leaves them NULL, passes. No row is
-- ever deleted.
CREATE FUNCTION season_stats_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA009', MESSAGE = 'season guard';
    END IF;
    IF current_setting('torneo.actor', true) IS DISTINCT FROM 'cli' THEN
        IF TG_OP = 'INSERT' THEN
            IF num_nonnulls(NEW.excluded_at, NEW.excluded_by, NEW.excluded_reason) <> 0 THEN
                RAISE EXCEPTION USING ERRCODE = 'TA009', MESSAGE = 'season guard';
            END IF;
        ELSIF ROW(NEW.excluded_at, NEW.excluded_by, NEW.excluded_reason)
              IS DISTINCT FROM ROW(OLD.excluded_at, OLD.excluded_by, OLD.excluded_reason) THEN
            RAISE EXCEPTION USING ERRCODE = 'TA009', MESSAGE = 'season guard';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER season_stats_guard BEFORE INSERT OR UPDATE OR DELETE ON season_stats
    FOR EACH ROW EXECUTE FUNCTION season_stats_guard();
CREATE TRIGGER season_stats_no_truncate BEFORE TRUNCATE ON season_stats
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();
