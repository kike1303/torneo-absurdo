//! 008 RF-B1, RF-B2 (ADR-0005): the ledger only grows, every entry balances, and the database,
//! not the application, guarantees both.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use server::ledger::{self, Coins, ISSUANCE_ACCOUNT, SettleEntry, SettleKind};
use sqlx::{PgPool, Row};
use support::bets;
use support::db::TestDb;
use support::fights as f;
use uuid::Uuid;

/// A user row with its ledger account, as the first login creates them.
async fn user(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO users (id, display_name, display_name_key) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(format!("Test{}", id.simple()))
        .bind(format!("test{}", id.simple()))
        .execute(&mut *tx)
        .await
        .unwrap();
    ledger::create_user_account(&mut tx, id).await.unwrap();
    tx.commit().await.unwrap();
    id
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned())
}

async fn total(pool: &PgPool) -> i64 {
    sqlx::query("SELECT COALESCE(SUM(amount), 0)::bigint FROM ledger_postings")
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn the_migrations_run_from_scratch_with_one_issuance_account() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let rows = sqlx::query("SELECT id FROM ledger_accounts WHERE kind = 'issuance'")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<Uuid, _>(0), ISSUANCE_ACCOUNT);
}

#[tokio::test]
async fn a_grant_moves_coins_from_issuance_and_the_total_stays_zero() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let b = user(&pool).await;
    let c = user(&pool).await;
    // The bounds of DAILY_GRANT, and a value in between.
    for (id, amount) in [(a, 100), (b, 1), (c, 10_000)] {
        let mut tx = pool.begin().await.unwrap();
        assert!(
            ledger::grant(&mut tx, id, Coins(amount))
                .await
                .unwrap()
                .is_some()
        );
        tx.commit().await.unwrap();
    }
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(100));
    assert_eq!(ledger::balance(&pool, b).await.unwrap(), Coins(1));
    assert_eq!(ledger::balance(&pool, c).await.unwrap(), Coins(10_000));
    // Conservation: only the issuance account went negative, by exactly what was granted.
    assert_eq!(total(&pool).await, 0);
    let issuance: i64 =
        sqlx::query("SELECT SUM(amount)::bigint FROM ledger_postings WHERE account_id = $1")
            .bind(ISSUANCE_ACCOUNT)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    assert_eq!(issuance, -10_101);
}

#[tokio::test]
async fn one_grant_per_user_and_utc_day() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    for expected in [true, false, false] {
        let mut tx = pool.begin().await.unwrap();
        let entry = ledger::grant(&mut tx, a, Coins(100)).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(entry.is_some(), expected);
    }
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(100));
    assert_eq!(total(&pool).await, 0);
}

#[tokio::test]
async fn update_delete_and_truncate_are_refused_with_ta001() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    ledger::grant(&mut tx, a, Coins(100)).await.unwrap();
    tx.commit().await.unwrap();
    for statement in [
        "UPDATE ledger_postings SET amount = amount * 2",
        "DELETE FROM ledger_postings",
        "TRUNCATE ledger_postings",
        "UPDATE ledger_entries SET kind = 'grant'",
        "DELETE FROM ledger_entries",
        "TRUNCATE ledger_entries CASCADE",
    ] {
        let error = sqlx::query(statement).execute(&pool).await.unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA001"), "{statement}");
        assert_eq!(server::db::error_kind(&error), "ledger_immutable");
    }
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(100));
}

/// Inserts an entry for `user` with these postings (account, amount) in one transaction.
async fn raw_entry(pool: &PgPool, user: Uuid, postings: &[(Uuid, i64)]) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let entry = Uuid::new_v4();
    sqlx::query("INSERT INTO ledger_entries (id, kind, user_id) VALUES ($1, 'grant', $2)")
        .bind(entry)
        .bind(user)
        .execute(&mut *tx)
        .await?;
    for (account, amount) in postings {
        sqlx::query(
            "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, $3)",
        )
        .bind(entry)
        .bind(account)
        .bind(amount)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn account_of(pool: &PgPool, user: Uuid) -> Uuid {
    sqlx::query("SELECT id FROM ledger_accounts WHERE user_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
async fn an_unbalanced_entry_or_one_with_fewer_than_two_postings_never_commits() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let account = account_of(&pool, a).await;
    for postings in [
        vec![(ISSUANCE_ACCOUNT, -100), (account, 99)],
        vec![(account, 100)],
        vec![],
    ] {
        let error = raw_entry(&pool, a, &postings).await.unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA002"), "{postings:?}");
        assert_eq!(server::db::error_kind(&error), "ledger_unbalanced");
    }
    let entries: i64 = sqlx::query("SELECT count(*) FROM ledger_entries")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(entries, 0);
    // A balanced one commits, and the failed attempts did not use the day's grant.
    raw_entry(&pool, a, &[(ISSUANCE_ACCOUNT, -100), (account, 100)])
        .await
        .unwrap();
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(100));
}

#[tokio::test]
async fn created_at_and_grant_day_from_the_application_are_ignored() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let account = account_of(&pool, a).await;
    let mut tx = pool.begin().await.unwrap();
    let entry = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, grant_day, created_at) \
         VALUES ($1, 'grant', $2, DATE '2000-01-01', TIMESTAMPTZ '2000-01-01 00:00:00Z')",
    )
    .bind(entry)
    .bind(a)
    .execute(&mut *tx)
    .await
    .unwrap();
    for (account, amount) in [(ISSUANCE_ACCOUNT, -5_i64), (account, 5)] {
        sqlx::query(
            "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, $3)",
        )
        .bind(entry)
        .bind(account)
        .bind(amount)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let row = sqlx::query(
        "SELECT grant_day = (now() AT TIME ZONE 'UTC')::date, created_at > now() - interval '1 minute' \
         FROM ledger_entries WHERE id = $1",
    )
    .bind(entry)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(row.get::<bool, _>(0));
    assert!(row.get::<bool, _>(1));
    // And so the app's grant for today now conflicts.
    let mut tx = pool.begin().await.unwrap();
    assert!(ledger::grant(&mut tx, a, Coins(5)).await.unwrap().is_none());
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn amounts_are_never_zero() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let account = account_of(&pool, a).await;
    let error = raw_entry(&pool, a, &[(ISSUANCE_ACCOUNT, 0), (account, 0)])
        .await
        .unwrap_err();
    // The CHECK fails at insert time, before the deferred balance check.
    assert_eq!(sqlstate(&error).as_deref(), Some("23514"));
}

/// Deviation 25 of plan-b, closed on #54: a committed entry takes no new postings at all. Not
/// even an offsetting pair on accounts it did not have (a retried payout reusing an entry id
/// would otherwise pay twice). Refused at insert with `TA003`.
async fn append(pool: &PgPool, entry: Uuid, postings: &[(Uuid, i64)]) -> sqlx::Error {
    let mut tx = pool.begin().await.unwrap();
    for (account, amount) in postings {
        let inserted = sqlx::query(
            "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, $3)",
        )
        .bind(entry)
        .bind(account)
        .bind(amount)
        .execute(&mut *tx)
        .await;
        if let Err(error) = inserted {
            return error;
        }
    }
    tx.commit().await.unwrap_err()
}

#[tokio::test]
async fn a_committed_entry_takes_no_new_postings() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let b = user(&pool).await;
    let c = user(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    let entry = ledger::grant(&mut tx, a, Coins(100))
        .await
        .unwrap()
        .unwrap();
    tx.commit().await.unwrap();
    let (account_b, account_c) = (account_of(&pool, b).await, account_of(&pool, c).await);
    for postings in [vec![(account_b, 5), (account_c, -5)], vec![(account_b, 50)]] {
        let error = append(&pool, entry, &postings).await;
        assert_eq!(sqlstate(&error).as_deref(), Some("TA003"), "{postings:?}");
        assert_eq!(server::db::error_kind(&error), "ledger_closed");
    }
    assert_eq!(ledger::balance(&pool, b).await.unwrap(), Coins(0));
    assert_eq!(ledger::balance(&pool, c).await.unwrap(), Coins(0));
    assert_eq!(total(&pool).await, 0);
}

/// `created_xact` is the top-level transaction id, so an entry built inside a savepoint (a
/// nested sqlx transaction), or finished after it, is still open in its own transaction.
#[tokio::test]
async fn an_entry_built_inside_a_savepoint_commits() {
    use sqlx::Connection;
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let account_a = account_of(&pool, a).await;
    let mut tx = pool.begin().await.unwrap();
    let entry = Uuid::new_v4();
    {
        let mut savepoint = (*tx).begin().await.unwrap();
        sqlx::query("INSERT INTO ledger_entries (id, kind, user_id) VALUES ($1, 'grant', $2)")
            .bind(entry)
            .bind(a)
            .execute(&mut *savepoint)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, 7)",
        )
        .bind(entry)
        .bind(account_a)
        .execute(&mut *savepoint)
        .await
        .unwrap();
        savepoint.commit().await.unwrap();
    }
    // The other half after the savepoint, in the same top-level transaction.
    sqlx::query("INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, -7)")
        .bind(entry)
        .bind(ISSUANCE_ACCOUNT)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(7));
}

/// Security nit on #54: the accounts are immutable too (an UPDATE of `user_id` would move a
/// whole balance to another user).
#[tokio::test]
async fn ledger_accounts_cannot_be_updated_or_deleted() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    user(&pool).await;
    for statement in [
        "UPDATE ledger_accounts SET user_id = $1 WHERE user_id IS NOT NULL",
        "DELETE FROM ledger_accounts WHERE user_id = $1",
    ] {
        let error = sqlx::query(statement)
            .bind(a)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA001"), "{statement}");
    }
    let error = sqlx::query("TRUNCATE ledger_accounts CASCADE")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&error).as_deref(), Some("TA001"));
}

/// The trigger, not the client, sets `created_xact`: a value the client supplies (here an old
/// transaction id, which would reopen nothing but break its own entry) is replaced with the
/// current top-level transaction.
#[tokio::test]
async fn created_xact_from_the_client_is_overwritten() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = user(&pool).await;
    let account_a = account_of(&pool, a).await;
    let mut tx = pool.begin().await.unwrap();
    let entry = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, created_xact) \
         VALUES ($1, 'grant', $2, '1'::xid8)",
    )
    .bind(entry)
    .bind(a)
    .execute(&mut *tx)
    .await
    .unwrap();
    let current: bool =
        sqlx::query("SELECT created_xact = pg_current_xact_id() FROM ledger_entries WHERE id = $1")
            .bind(entry)
            .fetch_one(&mut *tx)
            .await
            .unwrap()
            .get(0);
    assert!(current);
    for (account, amount) in [(account_a, 3_i64), (ISSUANCE_ACCOUNT, -3)] {
        sqlx::query(
            "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, $3)",
        )
        .bind(entry)
        .bind(account)
        .bind(amount)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

/// 012 RF2: the stake moves from the user to the escrow, and a bailout from the issuance, once a day.
#[tokio::test]
async fn a_stake_and_a_bailout_are_real_flows_of_the_rules() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = bets::user(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    let first = ledger::bailout(&mut tx, a, Coins(250)).await.unwrap();
    assert!(first.is_some());
    assert_eq!(ledger::bailout(&mut tx, a, Coins(250)).await.unwrap(), None);
    tx.commit().await.unwrap();
    let vector = f::Vector::of(f::KO);
    let fight = f::open_vector(&pool, f::T0, &vector).await;
    bets::seed_bet(&pool, fight, a, "a", 100).await;
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(150));
    assert_eq!(bets::escrow(&pool).await, 100);
    assert_eq!(total(&pool).await, 0);
}

/// 012 RF2: more rows than a batch, with every entry and its two postings, and nothing twice.
#[tokio::test]
async fn a_settlement_goes_by_batches() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let (a, b) = (bets::user(&pool).await, bets::user(&pool).await);
    let vector = f::Vector::of(f::KO);
    let fight = f::open_vector(&pool, f::T0, &vector).await;
    let count = ledger::SETTLE_BATCH_ROWS + 500;
    let bet_ids: Vec<Uuid> = (0..count).map(|_| Uuid::new_v4()).collect();
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         SELECT id, $1, CASE WHEN n % 2 = 0 THEN $2::uuid ELSE $3::uuid END, 'a', 10, true, \
         gen_random_uuid() FROM UNNEST($4::uuid[]) WITH ORDINALITY AS t(id, n)",
    )
    .bind(fight)
    .bind(a)
    .bind(b)
    .bind(&bet_ids)
    .execute(&pool)
    .await
    .unwrap();
    let entries: Vec<SettleEntry> = bet_ids
        .iter()
        .enumerate()
        .map(|(i, bet_id)| SettleEntry {
            kind: SettleKind::Refund,
            bet_id: *bet_id,
            user_id: if i % 2 == 1 { a } else { b },
            amount: Coins(10),
        })
        .collect();
    let mut tx = pool.begin().await.unwrap();
    ledger::settle_batch(&mut tx, fight, &entries)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_postings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, i64::try_from(count * 2).unwrap());
    assert_eq!(total(&pool).await, 0);
    assert_eq!(ledger::balance(&pool, a).await.unwrap(), Coins(7500));
    assert_eq!(ledger::balance(&pool, b).await.unwrap(), Coins(7500));
    // RF3: the same bet does not settle twice, and it says so by its index.
    let mut tx = pool.begin().await.unwrap();
    let error = ledger::settle_batch(&mut tx, fight, &entries[..1])
        .await
        .unwrap_err();
    assert!(server::db::is_unique_violation(
        &error,
        "ledger_entries_bet_settled_uq"
    ));
}

/// 012 RF2: a user without an account does not lose its half in silence: the entry does not commit.
#[tokio::test]
async fn a_settlement_of_a_user_without_an_account_never_commits() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let a = bets::user(&pool).await;
    let vector = f::Vector::of(f::KO);
    let fight = f::open_vector(&pool, f::T0, &vector).await;
    let bet = bets::seed_bet(&pool, fight, a, "a", 10).await;
    let stranger = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    let written = ledger::settle_batch(
        &mut tx,
        fight,
        &[SettleEntry {
            kind: SettleKind::Refund,
            bet_id: bet,
            user_id: stranger,
            amount: Coins(10),
        }],
    )
    .await;
    let failed = match written {
        Ok(()) => tx.commit().await.unwrap_err(),
        Err(error) => error,
    };
    assert!(failed.as_database_error().is_some());
    assert_eq!(bets::escrow(&pool).await, 10);
}
