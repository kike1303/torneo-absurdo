-- Reauthentication before link, unlink and admin (feature 009, TA-29: RF1 to RF5; plan decision
-- 1).
--
-- As in the earlier migrations, every constraint and trigger that can fail has a fixed name and
-- raises a fixed SQLSTATE with a fixed message and no values (ADR-0012 A9).

-- RF1: the link order. The rows that exist are numbered by (created_at, provider,
-- provider_user_id), not in their physical order (an `ADD COLUMN … GENERATED ALWAYS` would
-- number them as they lie on disk), and the sequence goes on above the maximum. This is the only
-- `setval` of the server (the source test of `admin.rs` checks every later file).
ALTER TABLE oauth_accounts ADD COLUMN id bigint;
UPDATE oauth_accounts o SET id = numbered.n
    FROM (
        SELECT provider, provider_user_id,
            row_number() OVER (ORDER BY created_at, provider, provider_user_id) AS n
        FROM oauth_accounts
    ) numbered
    WHERE o.provider = numbered.provider AND o.provider_user_id = numbered.provider_user_id;
ALTER TABLE oauth_accounts ALTER COLUMN id SET NOT NULL;
ALTER TABLE oauth_accounts ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY;
SELECT setval(
    pg_get_serial_sequence('oauth_accounts', 'id'),
    COALESCE((SELECT max(id) FROM oauth_accounts), 0) + 1,
    false
);
ALTER TABLE oauth_accounts ADD CONSTRAINT oauth_accounts_id_uq UNIQUE (id);

-- The second barrier of RF1 (plan, answer 3 of architect): `GENERATED ALWAYS` still allows
-- `UPDATE … SET id = DEFAULT`, which would give an old identity a new place in the order. No
-- exception, not even for the CLI. Created after the numbering above, which would trip it.
CREATE FUNCTION oauth_accounts_identity_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.user_id IS DISTINCT FROM OLD.user_id THEN
        RAISE EXCEPTION USING ERRCODE = 'TA005', MESSAGE = 'oauth account identity is immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER oauth_accounts_identity_guard BEFORE UPDATE OF id, user_id ON oauth_accounts
    FOR EACH ROW EXECUTE FUNCTION oauth_accounts_identity_guard();

-- RF2: what a session authenticated with. Rows of before this migration keep both in NULL: no
-- authentication that counts. The `SET NULL` of an unlink is an UPDATE (only `sessions::revoke`
-- deletes sessions, ADR-0008 D3), and the index keeps it from scanning every session.
ALTER TABLE sessions
    ADD COLUMN auth_account_id bigint NULL
        CONSTRAINT sessions_auth_account_fk REFERENCES oauth_accounts (id) ON DELETE SET NULL,
    ADD COLUMN auth_link_seq bigint NULL,
    ADD CONSTRAINT sessions_auth_ck CHECK (auth_account_id IS NULL OR auth_link_seq IS NOT NULL);
CREATE INDEX sessions_auth_account_id_idx ON sessions (auth_account_id);

-- RF3: the cutoff of `server admin grant`, a privileged column like `role` and `banned_at`.
ALTER TABLE users ADD COLUMN admin_account_cutoff bigint NULL;

CREATE OR REPLACE FUNCTION users_privileged_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF current_setting('torneo.actor', true) IS DISTINCT FROM 'cli' THEN
        IF TG_OP = 'INSERT' THEN
            IF NEW.role <> 'user' OR NEW.banned_at IS NOT NULL
                OR NEW.admin_account_cutoff IS NOT NULL THEN
                RAISE EXCEPTION USING ERRCODE = 'TA004', MESSAGE = 'privileged column write';
            END IF;
        ELSIF NEW.role IS DISTINCT FROM OLD.role OR NEW.banned_at IS DISTINCT FROM OLD.banned_at
            OR NEW.admin_account_cutoff IS DISTINCT FROM OLD.admin_account_cutoff THEN
            RAISE EXCEPTION USING ERRCODE = 'TA004', MESSAGE = 'privileged column write';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;
-- `UPDATE OF` names the cutoff too, so an UPDATE that only touches it fires the guard.
DROP TRIGGER users_privileged_guard ON users;
CREATE TRIGGER users_privileged_guard
    BEFORE INSERT OR UPDATE OF role, banned_at, admin_account_cutoff ON users
    FOR EACH ROW EXECUTE FUNCTION users_privileged_guard();

-- RF4: a `reauth` flow carries the user of the session that started it, as a `link` does.
ALTER TABLE oauth_flows
    DROP CONSTRAINT oauth_flows_purpose_ck,
    ADD CONSTRAINT oauth_flows_purpose_ck CHECK (purpose IN ('login', 'link', 'reauth')),
    DROP CONSTRAINT oauth_flows_link_ck,
    ADD CONSTRAINT oauth_flows_link_ck
        CHECK ((purpose IN ('link', 'reauth')) = (link_user_id IS NOT NULL));

-- RF5.
ALTER TABLE auth_events
    DROP CONSTRAINT auth_events_kind_ck,
    ADD CONSTRAINT auth_events_kind_ck
        CHECK (kind IN ('login', 'link', 'unlink', 'grant', 'reauth'));
