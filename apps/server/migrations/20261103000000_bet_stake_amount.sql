-- 012 RF2 and area of concern 21 (phase B): a `bet_stake` moves exactly the stake of its bet, from
-- its owner to the escrow, and nothing else. `BetsHook` pays from `bets.stake` without looking at the
-- entry, so a route that saved one amount and posted another would empty the escrow through the
-- legitimate settlement. Only that branch is new, and only it reads `bets`: refunds, payouts and the
-- rake, in the transaction of the reveal, read nothing more. The rest of the body is the one of
-- 20261102000000_bets.sql.
CREATE OR REPLACE FUNCTION ledger_posting_rules() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    account_kind text;
    account_user uuid;
    entry_kind text;
    entry_user uuid;
    entry_bet uuid;
    bet_stake bigint;
BEGIN
    SELECT kind, user_id INTO account_kind, account_user
        FROM ledger_accounts WHERE id = NEW.account_id;
    SELECT kind, user_id, bet_id INTO entry_kind, entry_user, entry_bet
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
    IF entry_kind = 'bet_stake' THEN
        SELECT stake INTO bet_stake FROM bets WHERE id = entry_bet;
        IF bet_stake IS NULL
           OR NOT ((account_kind = 'bet_escrow' AND NEW.amount = bet_stake)
                   OR (account_kind = 'user' AND NEW.amount = -bet_stake))
        THEN
            RAISE EXCEPTION USING ERRCODE = 'TA008', MESSAGE = 'ledger posting rule';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;
