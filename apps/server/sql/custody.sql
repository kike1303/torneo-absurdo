-- 012 RF13: the custody of the bets, read-only and in one statement. See apps/server/README.md.
SELECT
  (SELECT COALESCE(SUM(amount), 0)::bigint FROM ledger_postings
     WHERE account_id = '00000000-0000-0000-0000-000000000002') AS escrow,
  (SELECT COALESCE(SUM(b.stake), 0)::bigint FROM bets b
     JOIN fights f ON f.id = b.fight_id WHERE f.state <> 'settled') AS open_stakes,
  (SELECT COALESCE(SUM(amount), 0)::bigint FROM ledger_postings) AS all_movements,
  (SELECT count(*) FROM bets b WHERE NOT EXISTS (
     SELECT 1 FROM ledger_entries e
     JOIN ledger_postings p ON p.entry_id = e.id
       AND p.account_id = '00000000-0000-0000-0000-000000000002'
     WHERE e.bet_id = b.id AND e.kind = 'bet_stake' AND p.amount = b.stake)) AS bets_without_stake,
  (SELECT count(*) FROM ledger_entries e
     JOIN ledger_postings p ON p.entry_id = e.id
       AND p.account_id = '00000000-0000-0000-0000-000000000002'
     JOIN bets b ON b.id = e.bet_id
     WHERE e.kind = 'refund' AND -p.amount <> b.stake) AS wrong_refunds;
