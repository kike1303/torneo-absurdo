-- Authentication, sessions and the ledger (feature 008, phase B: RF-B1, RF-B2).
--
-- Every constraint and trigger that can fail has a fixed name, and the triggers raise a fixed
-- SQLSTATE with a fixed message and no values (ADR-0012 A9): an error never carries user data
-- into a log. `admin_audit` arrives with phase C.

CREATE TABLE users (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    display_name text NOT NULL,
    display_name_key text NOT NULL CONSTRAINT users_display_name_key_uq UNIQUE,
    display_name_is_generated boolean NOT NULL DEFAULT true,
    role text NOT NULL DEFAULT 'user' CONSTRAINT users_role_ck CHECK (role IN ('user', 'admin')),
    locale text NULL CONSTRAINT users_locale_ck CHECK (locale IN ('en', 'es')),
    theme text NULL CONSTRAINT users_theme_ck CHECK (theme IN ('dark', 'light', 'system')),
    created_at timestamptz NOT NULL DEFAULT now(),
    banned_at timestamptz NULL
);

-- `dev` is accepted in every environment: the release binary has no dev provider, and nobody can
-- insert such a row without database access (spec 008, area of concern 6).
CREATE TABLE oauth_accounts (
    provider text NOT NULL CONSTRAINT oauth_accounts_provider_ck
        CHECK (provider IN ('google', 'discord', 'dev')),
    provider_user_id text NOT NULL,
    user_id uuid NOT NULL REFERENCES users ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT oauth_accounts_pk PRIMARY KEY (provider, provider_user_id),
    CONSTRAINT oauth_accounts_user_provider_uq UNIQUE (user_id, provider)
);

CREATE TABLE oauth_flows (
    -- SHA-256 of the id in the flow cookie.
    id_hash bytea PRIMARY KEY,
    provider text NOT NULL,
    purpose text NOT NULL CONSTRAINT oauth_flows_purpose_ck CHECK (purpose IN ('login', 'link')),
    link_user_id uuid NULL REFERENCES users,
    state text NOT NULL,
    code_verifier text NOT NULL,
    next text NOT NULL,
    dev_identity text NULL,
    expires_at timestamptz NOT NULL,
    CONSTRAINT oauth_flows_link_ck CHECK ((purpose = 'link') = (link_user_id IS NOT NULL))
);
CREATE INDEX oauth_flows_expires_at_idx ON oauth_flows (expires_at);

CREATE TABLE sessions (
    -- Internal handle (socket registry, NOTIFY); never in a cookie.
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash bytea NOT NULL CONSTRAINT sessions_token_hash_uq UNIQUE,
    user_id uuid NOT NULL REFERENCES users,
    created_at timestamptz NOT NULL DEFAULT now(),
    authenticated_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    ip_hash bytea NOT NULL,
    user_agent_family text NULL CONSTRAINT sessions_user_agent_family_ck
        CHECK (user_agent_family IN ('chrome', 'firefox', 'safari', 'edge', 'other'))
);
CREATE INDEX sessions_user_id_idx ON sessions (user_id);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

-- The ledger (ADR-0005). No balance column: a user's balance is the SUM of its postings.
CREATE TABLE ledger_accounts (
    id uuid PRIMARY KEY,
    kind text NOT NULL CONSTRAINT ledger_accounts_kind_ck CHECK (kind IN ('user', 'issuance')),
    user_id uuid NULL CONSTRAINT ledger_accounts_user_uq UNIQUE REFERENCES users,
    CONSTRAINT ledger_accounts_user_ck CHECK ((kind = 'user') = (user_id IS NOT NULL))
);

-- The single issuance account: the source of every grant (fixed id, ledger::ISSUANCE_ACCOUNT).
INSERT INTO ledger_accounts (id, kind) VALUES ('00000000-0000-0000-0000-000000000001', 'issuance');

CREATE TABLE ledger_entries (
    id uuid PRIMARY KEY,
    -- TA-27 replaces this constraint with its new kinds, in its own migration.
    kind text NOT NULL CONSTRAINT ledger_entries_kind_ck CHECK (kind IN ('grant')),
    -- Null only for entries without a user (for example the rake of TA-27).
    user_id uuid NULL REFERENCES users,
    -- Both set by ledger_entry_defaults(), whatever the application sends.
    grant_day date NOT NULL,
    created_at timestamptz NOT NULL,
    CONSTRAINT ledger_entries_user_ck CHECK (kind NOT IN ('grant') OR user_id IS NOT NULL)
);
CREATE UNIQUE INDEX ledger_entries_grant_day_uq ON ledger_entries (user_id, grant_day)
    WHERE kind = 'grant';

CREATE TABLE ledger_postings (
    entry_id uuid NOT NULL REFERENCES ledger_entries,
    account_id uuid NOT NULL REFERENCES ledger_accounts,
    amount bigint NOT NULL CONSTRAINT ledger_postings_amount_ck CHECK (amount <> 0),
    CONSTRAINT ledger_postings_pk PRIMARY KEY (entry_id, account_id)
);
CREATE INDEX ledger_postings_account_id_idx ON ledger_postings (account_id);

CREATE FUNCTION ledger_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION USING ERRCODE = 'TA001', MESSAGE = 'ledger rows are immutable';
END;
$$;

CREATE TRIGGER ledger_entries_immutable BEFORE UPDATE OR DELETE ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER ledger_entries_no_truncate BEFORE TRUNCATE ON ledger_entries
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER ledger_postings_immutable BEFORE UPDATE OR DELETE ON ledger_postings
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER ledger_postings_no_truncate BEFORE TRUNCATE ON ledger_postings
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();

CREATE FUNCTION ledger_entry_defaults() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.created_at := now();
    NEW.grant_day := (now() AT TIME ZONE 'UTC')::date;
    RETURN NEW;
END;
$$;

CREATE TRIGGER ledger_entries_defaults BEFORE INSERT ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION ledger_entry_defaults();

-- At commit, every entry touched has at least two postings that add up to zero. On both tables,
-- so an entry without postings fails too (and does not use up the day's grant).
CREATE FUNCTION ledger_entry_balanced() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    entry uuid;
    postings bigint;
    total numeric;
BEGIN
    IF TG_TABLE_NAME = 'ledger_entries' THEN
        entry := NEW.id;
    ELSE
        entry := NEW.entry_id;
    END IF;
    SELECT count(*), COALESCE(sum(amount), 0) INTO postings, total
        FROM ledger_postings WHERE entry_id = entry;
    IF postings < 2 OR total <> 0 THEN
        RAISE EXCEPTION USING ERRCODE = 'TA002', MESSAGE = 'ledger entry is unbalanced';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER ledger_entries_balanced AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ledger_entry_balanced();
CREATE CONSTRAINT TRIGGER ledger_postings_balanced AFTER INSERT ON ledger_postings
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ledger_entry_balanced();

-- Login, link, unlink and grant signals (ADR-0008 D8). ip_hash only, never the IP. Kept 30 days.
CREATE TABLE auth_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users,
    kind text NOT NULL CONSTRAINT auth_events_kind_ck
        CHECK (kind IN ('login', 'link', 'unlink', 'grant')),
    provider text NULL CONSTRAINT auth_events_provider_ck
        CHECK (provider IN ('google', 'discord', 'dev')),
    ledger_entry_id uuid NULL REFERENCES ledger_entries,
    ip_hash bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX auth_events_created_at_idx ON auth_events (created_at);
