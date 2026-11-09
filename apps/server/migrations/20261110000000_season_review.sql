-- The review and the close of a season (feature 020, phase B; ADR-0008 D8). Additive (ADR-0006 D10).
--
-- `server season farm-report`, `exclude` and `close` audit themselves in `admin_audit`, attributed
-- to their admin. As in the earlier migrations, every constraint and trigger that can fail has a
-- fixed name and a fixed SQLSTATE with no values (ADR-0012 A9).

ALTER TABLE admin_audit
    DROP CONSTRAINT admin_audit_action_ck,
    ADD CONSTRAINT admin_audit_action_ck
        CHECK (action IN ('grant', 'revoke', 'ban', 'unban', 'fight_void',
                          'season_review', 'season_close', 'season_exclude'));

-- The second barrier of a review (the first is the source check of apps/server/src/admin/season.rs):
-- `close` trusts the newest one, so a row of `season_review` is refused unless an admin who is not
-- banned signs it from the CLI, for a season that exists and has already ended. Not a security
-- boundary (any SQL of the server could fill it in): it stops a review written by mistake or too
-- early.
CREATE FUNCTION admin_audit_season_review_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- Two steps: the cast of `target` runs only on six digits, whatever order `OR` takes.
    IF NEW.actor IS DISTINCT FROM 'cli_attributed'
       OR NEW.admin_id IS NULL
       OR NEW.target IS NULL
       OR NEW.target !~ '^[0-9]{6}$'
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA010', MESSAGE = 'season review guard';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM seasons WHERE id = NEW.target::int AND ends_at <= now())
       OR NOT EXISTS (SELECT 1 FROM users
                      WHERE id = NEW.admin_id AND role = 'admin' AND banned_at IS NULL)
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA010', MESSAGE = 'season review guard';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER admin_audit_season_review_guard BEFORE INSERT ON admin_audit
    FOR EACH ROW WHEN (NEW.action = 'season_review')
    EXECUTE FUNCTION admin_audit_season_review_guard();
