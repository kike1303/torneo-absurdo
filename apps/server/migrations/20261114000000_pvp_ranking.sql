-- The PvP ranking (feature 021, phase 4, TA-64): which fights moved the rating, the closed seasons
-- with the snapshot of their ratings, and the exclusions of the admin. Additive (ADR-0006 D10);
-- every constraint and trigger that can fail has a fixed name and a fixed SQLSTATE with no values
-- (ADR-0012 A9).

-- A fight is rated when it counts (no probation, pair under its limit) on a paid step of the day.
-- A rated draw of two equal ratings moves 0 and is still rated, so the flag and not the delta says
-- which fights a season counts. Every row of before this migration moved nothing.
--
-- Each side moves with its own K (owner, 2026-11-14), so the defender has its own change: opposite
-- in sign to the attacker's, or 0. Both are the changes as applied, within 0..4000.
ALTER TABLE pvp_fights
    ADD COLUMN rated boolean NOT NULL DEFAULT false,
    ADD COLUMN defender_rating_delta smallint NOT NULL DEFAULT 0
        CONSTRAINT pvp_fights_defender_rating_delta_ck
        CHECK (defender_rating_delta BETWEEN -40 AND 40),
    ADD CONSTRAINT pvp_fights_rated_ck
        CHECK (rated OR (rating_delta = 0 AND defender_rating_delta = 0)),
    ADD CONSTRAINT pvp_fights_rated_player_ck CHECK (defender_id IS NOT NULL OR NOT rated),
    ADD CONSTRAINT pvp_fights_rating_sides_ck CHECK (rating_delta * defender_rating_delta <= 0);

-- The body of 20261112000000_pvp_fights.sql with one more barrier: a rated fight never lands in a
-- season that is already closed. A fight that began in the last instant of a month and waited for
-- its close (which holds every botarga) would otherwise be rated in a month whose snapshot is taken;
-- it is refused instead, and the client retries in the new month (TA013).
CREATE OR REPLACE FUNCTION pvp_fights_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA011', MESSAGE = 'pvp fight guard';
    END IF;
    NEW.created_at := now();
    NEW.season_id := (extract(year FROM now() AT TIME ZONE 'UTC') * 100
                      + extract(month FROM now() AT TIME ZONE 'UTC'))::int;
    IF NEW.rated AND EXISTS (SELECT 1 FROM pvp_seasons WHERE id >= NEW.season_id) THEN
        RAISE EXCEPTION USING ERRCODE = 'TA013', MESSAGE = 'pvp ranking guard';
    END IF;
    RETURN NEW;
END;
$$;

-- The live leaderboard counts the rated fights of each side in the season, and the close reads the
-- rated fights after the season it closes.
CREATE INDEX pvp_fights_season_attacker_idx ON pvp_fights (season_id, attacker_id) WHERE rated;
CREATE INDEX pvp_fights_season_defender_idx ON pvp_fights (season_id, defender_id) WHERE rated;

-- A PvP season is the calendar month in UTC of `pvp_fights.season_id`. Its row is born closed, in
-- the transaction of the job that takes the snapshot of its ratings and applies the soft reset
-- (`pvp::season::close_due`), so a row means "closed" and never changes. The current month has no
-- row: its leaderboard is read live. `closed_at` is always the `now()` of that transaction.
CREATE TABLE pvp_seasons (
    id int PRIMARY KEY,
    starts_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL,
    closed_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pvp_seasons_month_ck CHECK (
        starts_at = date_trunc('month', starts_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
        AND ends_at = (date_trunc('month', starts_at AT TIME ZONE 'UTC') + interval '1 month')
            AT TIME ZONE 'UTC'
        AND id = extract(year FROM starts_at AT TIME ZONE 'UTC')::int * 100
            + extract(month FROM starts_at AT TIME ZONE 'UTC')::int
    ),
    CONSTRAINT pvp_seasons_closed_ck CHECK (closed_at >= ends_at)
);

-- The rating of each user with a rated fight in a closed season, before its soft reset, and the
-- rated fights, victories and defeats of that season.
CREATE TABLE pvp_season_ratings (
    season_id int NOT NULL REFERENCES pvp_seasons,
    user_id uuid NOT NULL REFERENCES botargas,
    rating int NOT NULL CONSTRAINT pvp_season_ratings_rating_ck CHECK (rating BETWEEN 0 AND 4000),
    rated_fights int NOT NULL CONSTRAINT pvp_season_ratings_fights_ck CHECK (rated_fights >= 1),
    wins int NOT NULL,
    losses int NOT NULL,
    CONSTRAINT pvp_season_ratings_pk PRIMARY KEY (season_id, user_id),
    CONSTRAINT pvp_season_ratings_results_ck
        CHECK (wins >= 0 AND losses >= 0 AND wins + losses <= rated_fights)
);
CREATE INDEX pvp_season_ratings_board_idx
    ON pvp_season_ratings (season_id, rating DESC, rated_fights DESC, user_id);

-- `server pvp exclude`: a user out of the leaderboard of a season, the current one included, so
-- the exclusion has its own table and does not wait for the close. The reasons are the closed list
-- of 020. No foreign key to `pvp_seasons`: the current season has no row.
CREATE TABLE pvp_exclusions (
    season_id int NOT NULL CONSTRAINT pvp_exclusions_season_ck
        CHECK (season_id / 100 BETWEEN 2000 AND 9999 AND season_id % 100 BETWEEN 1 AND 12),
    user_id uuid NOT NULL REFERENCES botargas,
    excluded_at timestamptz NOT NULL DEFAULT now(),
    excluded_by uuid NOT NULL REFERENCES users,
    reason text NOT NULL CONSTRAINT pvp_exclusions_reason_ck
        CHECK (reason IN ('farm_signals', 'shared_ip_opposite_sides', 'counterpart_concentration')),
    CONSTRAINT pvp_exclusions_pk PRIMARY KEY (season_id, user_id)
);

-- The three tables only grow, and each one from its own writer (TA013). As in 020, a barrier
-- against a write by mistake, not a security boundary:
-- - a season is closed once its month has ended;
-- - the ratings of a season only in the transaction that closes it (`closed_at` is that
--   transaction's `now()`), so a closed snapshot never changes;
-- - an exclusion only from the CLI (`torneo.actor = 'cli'`).
CREATE FUNCTION pvp_ranking_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- One branch per table: PL/pgSQL resolves every field of an expression, so `NEW.ends_at` may
    -- only be read where the row has it.
    IF TG_OP = 'INSERT' THEN
        IF TG_TABLE_NAME = 'pvp_seasons' THEN
            IF NEW.ends_at <= now() AND NEW.closed_at = now() THEN
                RETURN NEW;
            END IF;
        ELSIF TG_TABLE_NAME = 'pvp_season_ratings' THEN
            IF EXISTS (SELECT 1 FROM pvp_seasons WHERE id = NEW.season_id AND closed_at = now()) THEN
                RETURN NEW;
            END IF;
        ELSIF TG_TABLE_NAME = 'pvp_exclusions' THEN
            IF current_setting('torneo.actor', true) IS NOT DISTINCT FROM 'cli' THEN
                RETURN NEW;
            END IF;
        END IF;
    END IF;
    RAISE EXCEPTION USING ERRCODE = 'TA013', MESSAGE = 'pvp ranking guard';
END;
$$;
CREATE TRIGGER pvp_seasons_guard BEFORE INSERT OR UPDATE OR DELETE ON pvp_seasons
    FOR EACH ROW EXECUTE FUNCTION pvp_ranking_guard();
CREATE TRIGGER pvp_seasons_no_truncate BEFORE TRUNCATE ON pvp_seasons
    FOR EACH STATEMENT EXECUTE FUNCTION pvp_ranking_guard();
CREATE TRIGGER pvp_season_ratings_guard BEFORE INSERT OR UPDATE OR DELETE ON pvp_season_ratings
    FOR EACH ROW EXECUTE FUNCTION pvp_ranking_guard();
CREATE TRIGGER pvp_season_ratings_no_truncate BEFORE TRUNCATE ON pvp_season_ratings
    FOR EACH STATEMENT EXECUTE FUNCTION pvp_ranking_guard();
CREATE TRIGGER pvp_exclusions_guard BEFORE INSERT OR UPDATE OR DELETE ON pvp_exclusions
    FOR EACH ROW EXECUTE FUNCTION pvp_ranking_guard();
CREATE TRIGGER pvp_exclusions_no_truncate BEFORE TRUNCATE ON pvp_exclusions
    FOR EACH STATEMENT EXECUTE FUNCTION pvp_ranking_guard();

-- `server pvp farm-report` and `server pvp exclude` audit themselves, attributed to their admin.
ALTER TABLE admin_audit
    DROP CONSTRAINT admin_audit_action_ck,
    ADD CONSTRAINT admin_audit_action_ck
        CHECK (action IN ('grant', 'revoke', 'ban', 'unban', 'fight_void',
                          'season_review', 'season_close', 'season_exclude',
                          'pvp_review', 'pvp_exclude'));
