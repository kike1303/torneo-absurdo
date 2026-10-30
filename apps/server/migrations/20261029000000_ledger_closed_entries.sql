-- Closed ledger entries (feature 008, deviation 25 of plan-b, review of #54).
--
-- A committed entry takes no new postings, not even an offsetting pair: otherwise a retried or
-- buggy payout reusing an entry id would pay twice, and no unique index on ledger_entries would
-- stop it. Each entry records the top-level transaction that created it, and a posting is only
-- accepted in that same transaction. pg_current_xact_id() is the top-level id even inside a
-- savepoint (a nested sqlx transaction), so entries built in a savepoint still work. The
-- immutability trigger already forbids changing created_xact.
--
-- The accounts become immutable too: an UPDATE of user_id would move a whole balance.

ALTER TABLE ledger_entries
    ADD COLUMN created_xact xid8 NOT NULL DEFAULT pg_current_xact_id();

CREATE OR REPLACE FUNCTION ledger_entry_defaults() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.created_at := now();
    NEW.grant_day := (now() AT TIME ZONE 'UTC')::date;
    NEW.created_xact := pg_current_xact_id();
    RETURN NEW;
END;
$$;

CREATE FUNCTION ledger_posting_open_entry() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM ledger_entries
        WHERE id = NEW.entry_id AND created_xact = pg_current_xact_id()
    ) THEN
        RAISE EXCEPTION USING ERRCODE = 'TA003', MESSAGE = 'ledger entry is closed';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER ledger_postings_open_entry BEFORE INSERT ON ledger_postings
    FOR EACH ROW EXECUTE FUNCTION ledger_posting_open_entry();

CREATE TRIGGER ledger_accounts_immutable BEFORE UPDATE OR DELETE ON ledger_accounts
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER ledger_accounts_no_truncate BEFORE TRUNCATE ON ledger_accounts
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();
