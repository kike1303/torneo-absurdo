-- The bets (feature 012, phase A: RF1 to RF4, RF6, the immutability of rake_bp of RF10 and the audit
-- of RF18). Additive (ADR-0006 D10). `secret_lost` lives in its own migration, after this one, so
-- rejecting the amendment of ADR-0002 leaves this file as it is.
--
-- As in the earlier migrations, every constraint and trigger that can fail has a fixed name, and
-- every RAISE has a fixed SQLSTATE and message and no values (ADR-0012 A9).

-- RF1: the accounts of the bets, with fixed ids (ledger::ESCROW_ACCOUNT, ledger::RAKE_SINK_ACCOUNT).
ALTER TABLE ledger_accounts
    DROP CONSTRAINT ledger_accounts_kind_ck,
    ADD CONSTRAINT ledger_accounts_kind_ck
        CHECK (kind IN ('user', 'issuance', 'bet_escrow', 'rake_sink'));
INSERT INTO ledger_accounts (id, kind) VALUES ('00000000-0000-0000-0000-000000000002', 'bet_escrow');
INSERT INTO ledger_accounts (id, kind) VALUES ('00000000-0000-0000-0000-000000000003', 'rake_sink');

-- RF10: the rake of the fight, fixed when it is announced. Phase B writes it; until then, 0.
ALTER TABLE fights ADD COLUMN rake_bp smallint NOT NULL DEFAULT 0
    CONSTRAINT fights_rake_bp_ck CHECK (rake_bp BETWEEN 0 AND 2000);

-- RF4: insert-only. RF5: `ranked_stake` is a photo taken when the bet is created.
CREATE TABLE bets (
    id uuid PRIMARY KEY,
    fight_id int NOT NULL REFERENCES fights,
    user_id uuid NOT NULL REFERENCES users,
    side text NOT NULL CONSTRAINT bets_side_ck CHECK (side IN ('a', 'b')),
    stake bigint NOT NULL CONSTRAINT bets_stake_ck CHECK (stake > 0),
    ranked_stake boolean NOT NULL,
    client_key uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bets_client_key_uq UNIQUE (user_id, client_key),
    -- What `ledger_entries_bet_owner_fk` points to: an entry of a bet is of its owner and its fight.
    CONSTRAINT bets_owner_uq UNIQUE (id, user_id, fight_id)
);
CREATE INDEX bets_fight_user_idx ON bets (fight_id, user_id);
CREATE INDEX bets_user_fight_idx ON bets (user_id, fight_id);

-- RF6: the result of each bet. `season_stats` is of TA-28, which sums from `ranked_profit`.
CREATE TABLE bet_settlements (
    bet_id uuid PRIMARY KEY,
    fight_id int NOT NULL,
    user_id uuid NOT NULL,
    outcome text NOT NULL CONSTRAINT bet_settlements_outcome_ck
        CHECK (outcome IN ('won', 'lost', 'refunded')),
    payout bigint NOT NULL CONSTRAINT bet_settlements_payout_ck CHECK (payout >= 0),
    net bigint NOT NULL,
    ranked_profit bigint NOT NULL,
    -- The result of a bet is of its owner and its fight: TA-28 sums from here, and RF27 reads it.
    CONSTRAINT bet_settlements_owner_fk FOREIGN KEY (bet_id, user_id, fight_id)
        REFERENCES bets (id, user_id, fight_id),
    CONSTRAINT bet_settlements_ranked_ck CHECK (ranked_profit <= net),
    CONSTRAINT bet_settlements_refunded_ck
        CHECK (outcome <> 'refunded' OR (net = 0 AND ranked_profit = 0)),
    CONSTRAINT bet_settlements_lost_ck
        CHECK (outcome <> 'lost' OR (payout = 0 AND ranked_profit = net))
);
CREATE INDEX bet_settlements_user_fight_idx ON bet_settlements (user_id, fight_id)
    WHERE outcome <> 'refunded';

-- RF8: its primary key is what makes a settlement idempotent.
CREATE TABLE pool_settlements (
    fight_id int PRIMARY KEY REFERENCES fights,
    outcome text NOT NULL CONSTRAINT pool_settlements_outcome_ck
        CHECK (outcome IN ('paid', 'refund_one_sided', 'refund_draw', 'refund_voided',
                           'refund_cancelled')),
    pool_a bigint NOT NULL,
    pool_b bigint NOT NULL,
    rake_bp int NOT NULL,
    rake bigint NOT NULL,
    residue bigint NOT NULL,
    settled_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pool_settlements_refund_ck
        CHECK (outcome = 'paid' OR (rake_bp = 0 AND rake = 0 AND residue = 0))
);

CREATE TRIGGER bets_immutable BEFORE UPDATE OR DELETE ON bets
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER bets_no_truncate BEFORE TRUNCATE ON bets
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER bet_settlements_immutable BEFORE UPDATE OR DELETE ON bet_settlements
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER bet_settlements_no_truncate BEFORE TRUNCATE ON bet_settlements
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER pool_settlements_immutable BEFORE UPDATE OR DELETE ON pool_settlements
    FOR EACH ROW EXECUTE FUNCTION ledger_immutable();
CREATE TRIGGER pool_settlements_no_truncate BEFORE TRUNCATE ON pool_settlements
    FOR EACH STATEMENT EXECUTE FUNCTION ledger_immutable();

-- RF4: the second barrier of RF29 of 010. The first one is `lock_open_for_bet`.
CREATE FUNCTION bets_open_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM fights WHERE id = NEW.fight_id AND state = 'betting_open') THEN
        RAISE EXCEPTION USING ERRCODE = 'TA007', MESSAGE = 'the betting window is closed';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER bets_open_guard BEFORE INSERT ON bets
    FOR EACH ROW EXECUTE FUNCTION bets_open_guard();

-- RF1: the kinds of entry, what they must reference, and the columns RF3 looks at.
ALTER TABLE ledger_entries
    ADD COLUMN fight_id int NULL REFERENCES fights,
    ADD COLUMN bet_id uuid NULL,
    DROP CONSTRAINT ledger_entries_kind_ck,
    ADD CONSTRAINT ledger_entries_kind_ck
        CHECK (kind IN ('grant', 'bailout', 'bet_stake', 'bet_payout', 'refund', 'rake')),
    DROP CONSTRAINT ledger_entries_user_ck,
    -- A rake is of the fight, and of nobody: with a user, it could pay that user from the escrow.
    ADD CONSTRAINT ledger_entries_user_ck CHECK ((kind = 'rake') = (user_id IS NULL)),
    ADD CONSTRAINT ledger_entries_bet_refs_ck
        CHECK (kind NOT IN ('bet_stake', 'bet_payout', 'refund')
               OR (bet_id IS NOT NULL AND fight_id IS NOT NULL)),
    -- Only those three have a bet, so the key below always applies: a NULL would skip it.
    ADD CONSTRAINT ledger_entries_bet_kind_ck
        CHECK (bet_id IS NULL OR kind IN ('bet_stake', 'bet_payout', 'refund')),
    ADD CONSTRAINT ledger_entries_rake_fight_ck CHECK (kind <> 'rake' OR fight_id IS NOT NULL),
    -- A stake, a payout or a refund moves the coins of its own bet: of that user, in that fight.
    ADD CONSTRAINT ledger_entries_bet_owner_fk FOREIGN KEY (bet_id, user_id, fight_id)
        REFERENCES bets (id, user_id, fight_id);

-- RF3: one entry per fact, whatever the application does.
CREATE UNIQUE INDEX ledger_entries_bailout_day_uq ON ledger_entries (user_id, grant_day)
    WHERE kind = 'bailout';
CREATE UNIQUE INDEX ledger_entries_bet_stake_uq ON ledger_entries (bet_id)
    WHERE kind = 'bet_stake';
CREATE UNIQUE INDEX ledger_entries_bet_settled_uq ON ledger_entries (bet_id)
    WHERE kind IN ('bet_payout', 'refund');
CREATE UNIQUE INDEX ledger_entries_rake_uq ON ledger_entries (fight_id)
    WHERE kind = 'rake';

-- RF2: what each entry may move. Named so it runs after `ledger_postings_open_entry`: the BEFORE
-- triggers of a table run in the order of their names, and a closed entry stays TA003.
CREATE FUNCTION ledger_posting_rules() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    account_kind text;
    account_user uuid;
    entry_kind text;
    entry_user uuid;
BEGIN
    SELECT kind, user_id INTO account_kind, account_user
        FROM ledger_accounts WHERE id = NEW.account_id;
    SELECT kind, user_id INTO entry_kind, entry_user
        FROM ledger_entries WHERE id = NEW.entry_id;
    IF (account_kind = 'rake_sink' AND NEW.amount < 0)
       OR (account_kind = 'issuance' AND NEW.amount < 0 AND entry_kind NOT IN ('grant', 'bailout'))
       OR (account_kind = 'bet_escrow' AND NEW.amount > 0 AND entry_kind <> 'bet_stake')
       OR (account_kind = 'bet_escrow' AND NEW.amount < 0
           AND entry_kind NOT IN ('bet_payout', 'refund', 'rake'))
       OR (account_kind = 'user' AND account_user IS DISTINCT FROM entry_user)
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER ledger_postings_rules BEFORE INSERT ON ledger_postings
    FOR EACH ROW EXECUTE FUNCTION ledger_posting_rules();

-- RF18: the CLI of `fight void` attributes the command to an admin, so `admin_audit_cli_ck`, which
-- stays as it is, asks for an actor other than 'cli'.
ALTER TABLE admin_audit
    DROP CONSTRAINT admin_audit_actor_ck,
    ADD CONSTRAINT admin_audit_actor_ck CHECK (actor IN ('cli', 'cli_attributed')),
    DROP CONSTRAINT admin_audit_action_ck,
    ADD CONSTRAINT admin_audit_action_ck
        CHECK (action IN ('grant', 'revoke', 'ban', 'unban', 'fight_void'));

-- The events of phase B: the privacy notice already declares them.
ALTER TABLE auth_events
    ADD COLUMN bet_id uuid NULL REFERENCES bets,
    DROP CONSTRAINT auth_events_kind_ck,
    ADD CONSTRAINT auth_events_kind_ck
        CHECK (kind IN ('login', 'link', 'unlink', 'grant', 'reauth', 'bailout', 'bet'));

-- RF10: `rake_bp` joins the list of RF4 of 010. Only those two lists change.
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
