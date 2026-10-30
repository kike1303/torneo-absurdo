-- Admin audit and the guard of the privileged columns (feature 008, phase C: RF-C6; plan-c
-- decisions 10 and 11).
--
-- As in the earlier migrations, every constraint and trigger that can fail has a fixed name and
-- raises a fixed SQLSTATE with a fixed message and no values (ADR-0012 A9).

-- One row per command of the admin CLI. TA-13 and TA-27 replace `admin_audit_action_ck` (and
-- `admin_audit_actor_ck`, with the actor the guard below accepts) in their own migrations.
CREATE TABLE admin_audit (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor text NOT NULL CONSTRAINT admin_audit_actor_ck CHECK (actor IN ('cli')),
    admin_id uuid NULL REFERENCES users,
    action text NOT NULL CONSTRAINT admin_audit_action_ck
        CHECK (action IN ('grant', 'revoke', 'ban', 'unban')),
    target text NOT NULL,
    payload jsonb NOT NULL,
    at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT admin_audit_cli_ck CHECK ((actor = 'cli') = (admin_id IS NULL))
);
CREATE INDEX admin_audit_target_idx ON admin_audit (target, at);

-- Insert-only, with the trigger function of the ledger (spec 008, data model): TA001.
CREATE TRIGGER admin_audit_immutable BEFORE UPDATE OR DELETE ON admin_audit
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER admin_audit_no_truncate BEFORE TRUNCATE ON admin_audit
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();

-- The second barrier of RF-C6 (the first is the source check of apps/server/src/admin.rs): a
-- write of `role` or `banned_at` is refused unless the transaction set `torneo.actor = 'cli'`
-- with `SET LOCAL`, as the admin CLI does. Not a security boundary (any SQL of the server could
-- set it): it stops a dynamic UPDATE that the source check cannot see.
CREATE FUNCTION users_privileged_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF current_setting('torneo.actor', true) IS DISTINCT FROM 'cli' THEN
        IF TG_OP = 'INSERT' THEN
            IF NEW.role <> 'user' OR NEW.banned_at IS NOT NULL THEN
                RAISE EXCEPTION USING ERRCODE = 'TA004', MESSAGE = 'privileged column write';
            END IF;
        ELSIF NEW.role IS DISTINCT FROM OLD.role OR NEW.banned_at IS DISTINCT FROM OLD.banned_at THEN
            RAISE EXCEPTION USING ERRCODE = 'TA004', MESSAGE = 'privileged column write';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER users_privileged_guard BEFORE INSERT OR UPDATE OF role, banned_at ON users
    FOR EACH ROW EXECUTE FUNCTION users_privileged_guard();
