-- The shop (feature 024, phase 1, TA-92). Additive (ADR-0006 D10); every constraint and trigger
-- that can fail has a fixed name and a fixed SQLSTATE (ADR-0012 A9).
--
-- A purchase is one row of `shop_purchases`, one `shop_purchase` entry of the ledger (the user pays
-- the price, the shop sink takes it and never gives it back), one unlock and one event. The database
-- ties them: the entry moves exactly the price of its purchase, a purchase has exactly one entry,
-- and what was bought is never deleted.

-- The sink of the shop, like the rake sink: coins go in and never come out.
ALTER TABLE ledger_accounts
    DROP CONSTRAINT ledger_accounts_kind_ck,
    ADD CONSTRAINT ledger_accounts_kind_ck
        CHECK (kind IN ('user', 'issuance', 'bet_escrow', 'rake_sink', 'shop_sink'));
INSERT INTO ledger_accounts (id, kind) VALUES ('00000000-0000-0000-0000-000000000004', 'shop_sink');

-- One row per purchase, born whole and never changed (TA015). Nothing is bought twice, and a
-- `client_key` is one purchase.
CREATE TABLE shop_purchases (
    id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES botargas,
    kind text NOT NULL CONSTRAINT shop_purchases_kind_ck
        CHECK (kind IN ('accessory', 'cosmetic', 'skin')),
    item_id text NOT NULL CONSTRAINT shop_purchases_item_ck
        CHECK (item_id ~ '^[a-z0-9_-]{1,64}$'),
    price bigint NOT NULL CONSTRAINT shop_purchases_price_ck CHECK (price BETWEEN 1 AND 1000000),
    client_key uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT shop_purchases_item_uq UNIQUE (user_id, kind, item_id),
    CONSTRAINT shop_purchases_client_key_uq UNIQUE (user_id, client_key),
    CONSTRAINT shop_purchases_owner_uq UNIQUE (id, user_id)
);

CREATE FUNCTION shop_purchases_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA015', MESSAGE = 'shop guard';
    END IF;
    NEW.created_at := now();
    RETURN NEW;
END;
$$;
CREATE TRIGGER shop_purchases_guard BEFORE INSERT OR UPDATE OR DELETE ON shop_purchases
    FOR EACH ROW EXECUTE FUNCTION shop_purchases_guard();
CREATE TRIGGER shop_purchases_no_truncate BEFORE TRUNCATE ON shop_purchases
    FOR EACH STATEMENT EXECUTE FUNCTION shop_purchases_guard();

-- The entry of a purchase: one per purchase, of its owner.
ALTER TABLE ledger_entries
    ADD COLUMN shop_purchase_id int NULL,
    DROP CONSTRAINT ledger_entries_kind_ck,
    ADD CONSTRAINT ledger_entries_kind_ck
        CHECK (kind IN ('grant', 'bailout', 'bet_stake', 'bet_payout', 'refund', 'rake', 'pvp_reward',
                        'shop_purchase')),
    ADD CONSTRAINT ledger_entries_shop_ck
        CHECK ((kind = 'shop_purchase') = (shop_purchase_id IS NOT NULL)),
    ADD CONSTRAINT ledger_entries_shop_owner_fk FOREIGN KEY (shop_purchase_id, user_id)
        REFERENCES shop_purchases (id, user_id);
CREATE UNIQUE INDEX ledger_entries_shop_purchase_uq ON ledger_entries (shop_purchase_id)
    WHERE kind = 'shop_purchase';

-- At commit, every purchase has its entry: a purchase without a charge fails (TA016).
CREATE FUNCTION shop_purchase_charged() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM ledger_entries WHERE kind = 'shop_purchase' AND shop_purchase_id = NEW.id
    ) THEN
        RAISE EXCEPTION USING ERRCODE = 'TA016', MESSAGE = 'shop purchase without a charge';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER shop_purchases_charged AFTER INSERT ON shop_purchases
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION shop_purchase_charged();

-- The body of 20261112000000_pvp_fights.sql with two changes: the shop sink only takes positive
-- amounts and only from a `shop_purchase`, and a `shop_purchase` moves exactly the price of its
-- purchase, from its owner to the sink, and nothing else. The issuance still never pays one.
CREATE OR REPLACE FUNCTION ledger_posting_rules() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    account_kind text;
    account_user uuid;
    entry_kind text;
    entry_user uuid;
    entry_bet uuid;
    entry_pvp int;
    entry_shop int;
    bet_stake bigint;
    pvp_coins bigint;
    shop_price bigint;
BEGIN
    SELECT kind, user_id INTO account_kind, account_user
        FROM ledger_accounts WHERE id = NEW.account_id;
    SELECT kind, user_id, bet_id, pvp_fight_id, shop_purchase_id
        INTO entry_kind, entry_user, entry_bet, entry_pvp, entry_shop
        FROM ledger_entries WHERE id = NEW.entry_id;
    IF (account_kind = 'rake_sink' AND NEW.amount < 0)
       OR (account_kind = 'shop_sink' AND (NEW.amount < 0 OR entry_kind <> 'shop_purchase'))
       OR (account_kind = 'issuance' AND NEW.amount < 0
           AND entry_kind NOT IN ('grant', 'bailout', 'pvp_reward'))
       OR (account_kind = 'bet_escrow' AND NEW.amount > 0 AND entry_kind <> 'bet_stake')
       OR (account_kind = 'bet_escrow' AND NEW.amount < 0
           AND entry_kind NOT IN ('bet_payout', 'refund', 'rake'))
       OR (account_kind = 'user' AND account_user IS DISTINCT FROM entry_user)
    THEN
        RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
    END IF;
    IF entry_kind = 'bet_stake' THEN
        SELECT stake INTO bet_stake FROM bets WHERE id = entry_bet;
        IF bet_stake IS NULL
           OR NOT ((account_kind = 'bet_escrow' AND NEW.amount = bet_stake)
                   OR (account_kind = 'user' AND NEW.amount = -bet_stake))
        THEN
            RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
        END IF;
    END IF;
    IF entry_kind = 'pvp_reward' THEN
        SELECT coins_attacker INTO pvp_coins FROM pvp_fights WHERE id = entry_pvp;
        IF pvp_coins IS NULL
           OR NOT ((account_kind = 'issuance' AND NEW.amount = -pvp_coins)
                   OR (account_kind = 'user' AND NEW.amount = pvp_coins))
        THEN
            RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
        END IF;
    END IF;
    IF entry_kind = 'shop_purchase' THEN
        SELECT price INTO shop_price FROM shop_purchases WHERE id = entry_shop;
        IF shop_price IS NULL
           OR NOT ((account_kind = 'shop_sink' AND NEW.amount = shop_price)
                   OR (account_kind = 'user' AND NEW.amount = -shop_price))
        THEN
            RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

-- What was bought joins the unlocks, and is never taken away (TA015).
ALTER TABLE botarga_unlocks
    DROP CONSTRAINT botarga_unlocks_kind_ck,
    ADD CONSTRAINT botarga_unlocks_kind_ck
        CHECK (kind IN ('ability', 'accessory', 'cosmetic', 'skin')),
    DROP CONSTRAINT botarga_unlocks_source_ck,
    ADD CONSTRAINT botarga_unlocks_source_ck
        CHECK (source IN ('starter', 'draw', 'level', 'torneo', 'shop'));

CREATE FUNCTION botarga_unlocks_shop_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.source = 'shop' THEN
        RAISE EXCEPTION USING ERRCODE = 'TA015', MESSAGE = 'shop guard';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER botarga_unlocks_shop_guard BEFORE UPDATE OR DELETE ON botarga_unlocks
    FOR EACH ROW EXECUTE FUNCTION botarga_unlocks_shop_guard();

-- One event per purchase, with its entry and the `ip_hash` of the request (retention of D8). The
-- lasting record is `shop_purchases` and the ledger.
ALTER TABLE auth_events
    DROP CONSTRAINT auth_events_kind_ck,
    ADD CONSTRAINT auth_events_kind_ck
        CHECK (kind IN ('login', 'link', 'unlink', 'grant', 'reauth', 'bailout', 'bet', 'pvp_attack',
                        'friend_challenge', 'friend_tournament', 'friend_accept', 'purchase')),
    ADD CONSTRAINT auth_events_purchase_ck CHECK (kind <> 'purchase' OR ledger_entry_id IS NOT NULL);
