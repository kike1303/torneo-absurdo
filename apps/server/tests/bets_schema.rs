//! 012 RF1 to RF4, RF6, the immutability of `rake_bp` of RF10 and the audit of RF18: what the
//! database alone enforces, with direct SQL and one case per rule.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use server::ledger::{ESCROW_ACCOUNT, ISSUANCE_ACCOUNT, RAKE_SINK_ACCOUNT};
use sqlx::{AssertSqlSafe, PgPool, Row};
use support::db::TestDb;
use support::fights::{self as f, KO, T0, Vector};
use uuid::Uuid;

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned())
}

fn constraint(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|e| e.constraint())
        .map(str::to_owned)
}

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
    server::ledger::create_user_account(&mut tx, id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn account(pool: &PgPool, user: Uuid) -> Uuid {
    sqlx::query("SELECT id FROM ledger_accounts WHERE user_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// A bet of `who`, by SQL: the table has its own rules, whatever writes it.
async fn bet(pool: &PgPool, fight: i32, who: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
         VALUES ($1, $2, $3, 'a', 10, true, $4)",
    )
    .bind(id)
    .bind(fight)
    .bind(who)
    .bind(Uuid::new_v4())
    .execute(pool)
    .await
    .unwrap();
    id
}

struct Entry {
    kind: &'static str,
    user: Option<Uuid>,
    fight: Option<i32>,
    bet: Option<Uuid>,
    postings: Vec<(Uuid, i64)>,
}

async fn insert(pool: &PgPool, entry: &Entry) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ledger_entries (id, kind, user_id, fight_id, bet_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(entry.kind)
    .bind(entry.user)
    .bind(entry.fight)
    .bind(entry.bet)
    .execute(&mut *tx)
    .await?;
    for (account, amount) in &entry.postings {
        sqlx::query(
            "INSERT INTO ledger_postings (entry_id, account_id, amount) VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(account)
        .bind(amount)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// A database, a user with its account, and an open fight with a bet of that user.
struct World {
    _db: TestDb,
    pool: PgPool,
    user: Uuid,
    account: Uuid,
    fight: i32,
    bet: Uuid,
}

async fn world() -> World {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let who = user(&pool).await;
    let account = account(&pool, who).await;
    let vector = Vector::of(KO);
    let fight = f::open_vector(&pool, T0, &vector).await;
    let bet = bet(&pool, fight, who).await;
    World {
        _db: db,
        pool,
        user: who,
        account,
        fight,
        bet,
    }
}

impl World {
    /// A `grant` or a `bailout`: of a user, with no fight and no bet.
    fn plain(&self, kind: &'static str, postings: &[(Uuid, i64)]) -> Entry {
        Entry {
            fight: None,
            bet: None,
            ..self.entry(kind, postings)
        }
    }

    fn entry(&self, kind: &'static str, postings: &[(Uuid, i64)]) -> Entry {
        Entry {
            kind,
            user: Some(self.user),
            fight: Some(self.fight),
            bet: Some(self.bet),
            postings: postings.to_vec(),
        }
    }
}

/// RF1: every kind is accepted with what it must reference, and its accounts exist.
#[tokio::test]
async fn every_kind_of_entry_is_accepted() {
    let w = world().await;
    let (user_up, user_down) = (
        [(ISSUANCE_ACCOUNT, -5), (w.account, 5)],
        [(w.account, -5), (ESCROW_ACCOUNT, 5)],
    );
    insert(&w.pool, &w.plain("grant", &user_up)).await.unwrap();
    insert(&w.pool, &w.plain("bailout", &user_up))
        .await
        .unwrap();
    insert(&w.pool, &w.entry("bet_stake", &user_down))
        .await
        .unwrap();
    insert(
        &w.pool,
        &w.entry("bet_payout", &[(ESCROW_ACCOUNT, -5), (w.account, 5)]),
    )
    .await
    .unwrap();
    let other = bet(&w.pool, w.fight, w.user).await;
    insert(
        &w.pool,
        &Entry {
            bet: Some(other),
            ..w.entry("refund", &[(ESCROW_ACCOUNT, -5), (w.account, 5)])
        },
    )
    .await
    .unwrap();
    insert(
        &w.pool,
        &Entry {
            user: None,
            bet: None,
            ..w.entry("rake", &[(ESCROW_ACCOUNT, -5), (RAKE_SINK_ACCOUNT, 5)])
        },
    )
    .await
    .unwrap();
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM ledger_accounts WHERE user_id IS NULL ORDER BY id")
            .fetch_all(&w.pool)
            .await
            .unwrap();
    assert_eq!(kinds, ["issuance", "bet_escrow", "rake_sink"]);
}

/// RF1: without these columns, the indexes of RF3 would not see the entry.
#[tokio::test]
async fn an_entry_without_what_it_references_fails() {
    let w = world().await;
    let postings = [(ESCROW_ACCOUNT, -5), (w.account, 5)];
    let error = insert(&w.pool, &w.plain("other", &postings))
        .await
        .unwrap_err();
    assert_eq!(
        constraint(&error).as_deref(),
        Some("ledger_entries_kind_ck")
    );
    for kind in ["bet_stake", "bet_payout", "refund"] {
        for entry in [
            Entry {
                bet: None,
                ..w.entry(kind, &postings)
            },
            Entry {
                fight: None,
                ..w.entry(kind, &postings)
            },
        ] {
            let error = insert(&w.pool, &entry).await.unwrap_err();
            assert_eq!(
                constraint(&error).as_deref(),
                Some("ledger_entries_bet_refs_ck"),
                "{kind}"
            );
        }
    }
    let rake = [(ESCROW_ACCOUNT, -5), (RAKE_SINK_ACCOUNT, 5)];
    let error = insert(
        &w.pool,
        &Entry {
            user: None,
            fight: None,
            bet: None,
            ..w.entry("rake", &rake)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        constraint(&error).as_deref(),
        Some("ledger_entries_rake_fight_ck")
    );
    // Only a rake goes without a user.
    let error = insert(
        &w.pool,
        &Entry {
            user: None,
            ..w.entry("refund", &postings)
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        constraint(&error).as_deref(),
        Some("ledger_entries_user_ck")
    );
}

/// The database ties an entry to its bet: with direct SQL, a rake or a refund cannot pay somebody from
/// the escrow with the bet of another (review of PR #86).
#[tokio::test]
async fn an_entry_only_moves_the_coins_of_its_own_bet() {
    let w = world().await;
    let attacker = user(&w.pool).await;
    let attacker_account = account(&w.pool, attacker).await;
    let down = [(ESCROW_ACCOUNT, -1005), (attacker_account, 1005)];
    let stake = [(attacker_account, -5), (ESCROW_ACCOUNT, 5)];
    let own_fight = f::open_vector(&w.pool, T0 + f::CADENCE, &Vector::of(KO)).await;
    let own_bet = bet(&w.pool, own_fight, attacker).await;
    for (case, entry, name) in [
        (
            "a refund of the bet of another",
            Entry {
                user: Some(attacker),
                ..w.entry("refund", &down)
            },
            "ledger_entries_bet_owner_fk",
        ),
        (
            "a payout of the bet of another",
            Entry {
                user: Some(attacker),
                ..w.entry("bet_payout", &down)
            },
            "ledger_entries_bet_owner_fk",
        ),
        (
            "a stake on the bet of another",
            Entry {
                user: Some(attacker),
                ..w.entry("bet_stake", &stake)
            },
            "ledger_entries_bet_owner_fk",
        ),
        (
            "a refund of its own bet, in another fight",
            Entry {
                user: Some(attacker),
                bet: Some(own_bet),
                ..w.entry("refund", &down)
            },
            "ledger_entries_bet_owner_fk",
        ),
        (
            "a bet that does not exist",
            Entry {
                bet: Some(Uuid::new_v4()),
                ..w.entry("refund", &down)
            },
            "ledger_entries_bet_owner_fk",
        ),
        (
            "a rake that pays a user",
            Entry {
                user: Some(attacker),
                bet: None,
                ..w.entry("rake", &down)
            },
            "ledger_entries_user_ck",
        ),
        (
            "a rake with a bet",
            Entry {
                user: None,
                ..w.entry("rake", &[(ESCROW_ACCOUNT, -5), (RAKE_SINK_ACCOUNT, 5)])
            },
            "ledger_entries_bet_kind_ck",
        ),
        (
            "a grant with a bet",
            Entry {
                user: Some(attacker),
                ..w.entry("grant", &[(ISSUANCE_ACCOUNT, -5), (attacker_account, 5)])
            },
            "ledger_entries_bet_kind_ck",
        ),
    ] {
        let error = insert(&w.pool, &entry).await.expect_err(case);
        assert_eq!(constraint(&error).as_deref(), Some(name), "{case}");
    }
    let moved: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_postings")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(moved, 0);
    // Its own refund, in its own fight, is what is left.
    insert(
        &w.pool,
        &Entry {
            user: Some(attacker),
            fight: Some(own_fight),
            bet: Some(own_bet),
            ..w.entry("refund", &[(ESCROW_ACCOUNT, -10), (attacker_account, 10)])
        },
    )
    .await
    .unwrap();
}

/// RF2: one refusal per rule, and each with `TA008` before the commit.
#[tokio::test]
async fn each_rule_of_the_postings_refuses_with_its_code() {
    let w = world().await;
    let other = user(&w.pool).await;
    let other_account = account(&w.pool, other).await;
    let cases: Vec<(&str, Entry)> = vec![
        (
            "the sink is never debited",
            Entry {
                user: None,
                bet: None,
                ..w.entry("rake", &[(RAKE_SINK_ACCOUNT, -5), (ESCROW_ACCOUNT, 5)])
            },
        ),
        (
            "the issuance is only debited in a grant or a bailout",
            w.entry("refund", &[(ISSUANCE_ACCOUNT, -5), (w.account, 5)]),
        ),
        (
            "the issuance is not debited in a payout",
            w.entry("bet_payout", &[(ISSUANCE_ACCOUNT, -5), (w.account, 5)]),
        ),
        (
            "only a stake credits the escrow",
            w.entry("refund", &[(w.account, -5), (ESCROW_ACCOUNT, 5)]),
        ),
        (
            "the escrow is not debited in a grant",
            w.plain("grant", &[(ESCROW_ACCOUNT, -5), (w.account, 5)]),
        ),
        (
            "the escrow is not debited in a bailout",
            w.plain("bailout", &[(ESCROW_ACCOUNT, -5), (w.account, 5)]),
        ),
        (
            "a user account belongs to the user of the entry",
            w.entry("refund", &[(ESCROW_ACCOUNT, -5), (other_account, 5)]),
        ),
        (
            "and a rake has no user of its own",
            Entry {
                user: None,
                bet: None,
                ..w.entry("rake", &[(ESCROW_ACCOUNT, -5), (w.account, 5)])
            },
        ),
    ];
    for (rule, entry) in &cases {
        let error = insert(&w.pool, entry).await.unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA008"), "{rule}");
        assert_eq!(server::db::error_kind(&error), "ledger_posting_rule");
    }
}

/// RF3: a second entry of each class fails, by the database.
#[tokio::test]
async fn one_entry_per_fact() {
    let w = world().await;
    let (up, stake, down) = (
        [(ISSUANCE_ACCOUNT, -5), (w.account, 5)],
        [(w.account, -5), (ESCROW_ACCOUNT, 5)],
        [(ESCROW_ACCOUNT, -5), (w.account, 5)],
    );
    let rake = [(ESCROW_ACCOUNT, -5), (RAKE_SINK_ACCOUNT, 5)];
    let rake_entry = Entry {
        user: None,
        bet: None,
        ..w.entry("rake", &rake)
    };
    for (first, second, index) in [
        (
            w.plain("bailout", &up),
            w.plain("bailout", &up),
            "ledger_entries_bailout_day_uq",
        ),
        (
            w.entry("bet_stake", &stake),
            w.entry("bet_stake", &stake),
            "ledger_entries_bet_stake_uq",
        ),
        (
            w.entry("refund", &down),
            w.entry("bet_payout", &down),
            "ledger_entries_bet_settled_uq",
        ),
    ] {
        insert(&w.pool, &first).await.unwrap();
        let error = insert(&w.pool, &second).await.unwrap_err();
        assert_eq!(constraint(&error).as_deref(), Some(index));
    }
    insert(&w.pool, &rake_entry).await.unwrap();
    let error = insert(&w.pool, &rake_entry).await.unwrap_err();
    assert_eq!(
        constraint(&error).as_deref(),
        Some("ledger_entries_rake_uq")
    );
}

async fn settlement(pool: &PgPool, w: &World) {
    sqlx::query(
        "INSERT INTO bet_settlements (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
         VALUES ($1, $2, $3, 'refunded', 10, 0, 0)",
    )
    .bind(w.bet)
    .bind(w.fight)
    .bind(w.user)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO pool_settlements (fight_id, outcome, pool_a, pool_b, rake_bp, rake, residue) \
         VALUES ($1, 'refund_voided', 10, 0, 0, 0, 0)",
    )
    .bind(w.fight)
    .execute(pool)
    .await
    .unwrap();
}

/// RF4 and RF6: the three tables only grow.
#[tokio::test]
async fn the_tables_of_the_bets_are_insert_only() {
    let w = world().await;
    settlement(&w.pool, &w).await;
    for sql in [
        "UPDATE bets SET stake = 1",
        "DELETE FROM bets",
        "TRUNCATE bets CASCADE",
        "UPDATE bet_settlements SET payout = 1",
        "DELETE FROM bet_settlements",
        "TRUNCATE bet_settlements",
        "UPDATE pool_settlements SET rake = 1",
        "DELETE FROM pool_settlements",
        "TRUNCATE pool_settlements",
    ] {
        let error = sqlx::query(AssertSqlSafe(sql))
            .execute(&w.pool)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA001"), "{sql}");
    }
}

/// RF4: the second barrier of RF29 of 010, in every state but `betting_open`.
#[tokio::test]
async fn a_bet_needs_an_open_window() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let who = user(&pool).await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let attempt = |pool: PgPool| async move {
        sqlx::query(
            "INSERT INTO bets (id, fight_id, user_id, side, stake, ranked_stake, client_key) \
             VALUES ($1, $2, $3, 'a', 10, true, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(who)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
    };
    let closed = |error: sqlx::Error| {
        assert_eq!(sqlstate(&error).as_deref(), Some("TA007"));
        assert_eq!(server::db::error_kind(&error), "bet_window_closed");
    };
    closed(attempt(pool.clone()).await.unwrap_err());
    f::open_with(&pool, id, &vector.seed, T0).await;
    attempt(pool.clone()).await.unwrap();
    f::close_by_sql(&pool, id, T0 + f::WINDOW).await;
    closed(attempt(pool.clone()).await.unwrap_err());
    f::live_by_sql(&pool, id, T0 + f::WINDOW + f::PRE_LIVE).await;
    closed(attempt(pool.clone()).await.unwrap_err());
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, id, &vector.seed, "engine_error").await;
    tx.commit().await.unwrap();
    closed(attempt(pool.clone()).await.unwrap_err());
}

/// RF6: each `CHECK` of the result of a bet, and of the result of its pool.
#[tokio::test]
async fn the_result_of_a_bet_has_its_rules() {
    let w = world().await;
    for (outcome, payout, net, ranked, name) in [
        ("won", 20, 10, 11, "bet_settlements_ranked_ck"),
        ("refunded", 10, 1, 0, "bet_settlements_refunded_ck"),
        ("refunded", 10, 0, -1, "bet_settlements_refunded_ck"),
        ("lost", 1, -10, -10, "bet_settlements_lost_ck"),
        ("lost", 0, -10, -11, "bet_settlements_lost_ck"),
        ("won", -1, 10, 0, "bet_settlements_payout_ck"),
        ("other", 0, 0, 0, "bet_settlements_outcome_ck"),
    ] {
        let error = sqlx::query(
            "INSERT INTO bet_settlements \
             (bet_id, fight_id, user_id, outcome, payout, net, ranked_profit) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(w.bet)
        .bind(w.fight)
        .bind(w.user)
        .bind(outcome)
        .bind(i64::from(payout))
        .bind(i64::from(net))
        .bind(i64::from(ranked))
        .execute(&w.pool)
        .await
        .unwrap_err();
        assert_eq!(constraint(&error).as_deref(), Some(name), "{outcome}");
    }
    for (outcome, rake_bp, rake, residue) in [
        ("refund_voided", 500, 0, 0),
        ("refund_draw", 0, 1, 0),
        ("refund_one_sided", 0, 0, 1),
    ] {
        let error = sqlx::query(
            "INSERT INTO pool_settlements (fight_id, outcome, pool_a, pool_b, rake_bp, rake, residue) \
             VALUES ($1, $2, 1, 1, $3, $4, $5)",
        )
        .bind(w.fight)
        .bind(outcome)
        .bind(rake_bp)
        .bind(i64::from(rake))
        .bind(i64::from(residue))
        .execute(&w.pool)
        .await
        .unwrap_err();
        assert_eq!(
            constraint(&error).as_deref(),
            Some("pool_settlements_refund_ck"),
            "{outcome}"
        );
    }
    settlement(&w.pool, &w).await;
    // RF8: the idempotence of a settlement is its primary key.
    let again = sqlx::query_scalar::<_, i32>(
        "INSERT INTO pool_settlements (fight_id, outcome, pool_a, pool_b, rake_bp, rake, residue) \
         VALUES ($1, 'refund_draw', 1, 1, 0, 0, 0) ON CONFLICT (fight_id) DO NOTHING RETURNING fight_id",
    )
    .bind(w.fight)
    .fetch_optional(&w.pool)
    .await
    .unwrap();
    assert_eq!(again, None);
}

/// RF10, with every migration applied: the rake of a fight never changes, in any state.
#[tokio::test]
async fn the_rake_of_a_fight_is_immutable() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let vector = Vector::of(KO);
    let id = f::announce(&pool, T0, &vector.a, &vector.b).await;
    let refused = |pool: PgPool| async move {
        let error = sqlx::query("UPDATE fights SET rake_bp = 500 WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&error).as_deref(), Some("TA006"));
    };
    refused(pool.clone()).await;
    f::open_with(&pool, id, &vector.seed, T0).await;
    refused(pool.clone()).await;
    f::close_by_sql(&pool, id, T0 + f::WINDOW).await;
    refused(pool.clone()).await;
    f::live_by_sql(&pool, id, T0 + f::WINDOW + f::PRE_LIVE).await;
    refused(pool.clone()).await;
    let mut tx = pool.begin().await.unwrap();
    f::void_by_sql(&mut tx, id, &vector.seed, "engine_error").await;
    tx.commit().await.unwrap();
    refused(pool.clone()).await;

    // And an edge of the graph does not carry a change with it.
    let other = f::announce(&pool, T0 + 1000, &vector.a, &vector.b).await;
    f::open_with(&pool, other, &vector.seed, T0 + 1000).await;
    let error = sqlx::query(
        "UPDATE fights SET state = 'betting_closed', closed_at = now(), rake_bp = 500 WHERE id = $1",
    )
    .bind(other)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(sqlstate(&error).as_deref(), Some("TA006"));
    // It defaults to 0, and has its range.
    let rake: i16 = sqlx::query_scalar("SELECT rake_bp FROM fights WHERE id = $1")
        .bind(other)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rake, 0);
}

/// RF18: the actor of a command attributed to an admin, and `admin_audit_cli_ck` as it was.
#[tokio::test]
async fn the_audit_of_a_void_names_its_admin() {
    let db = TestDb::create().await;
    let pool = db.migrated_pool().await;
    let admin = user(&pool).await;
    let insert = |actor: &'static str, admin: Option<Uuid>, action: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO admin_audit (actor, admin_id, action, target, payload) \
                 VALUES ($1, $2, $3, '1', '{}'::jsonb)",
            )
            .bind(actor)
            .bind(admin)
            .bind(action)
            .execute(&pool)
            .await
        }
    };
    insert("cli_attributed", Some(admin), "fight_void")
        .await
        .unwrap();
    for (actor, who, action, name) in [
        ("cli_attributed", None, "fight_void", "admin_audit_cli_ck"),
        ("cli", Some(admin), "fight_void", "admin_audit_cli_ck"),
        ("web", Some(admin), "fight_void", "admin_audit_actor_ck"),
        (
            "cli_attributed",
            Some(admin),
            "other",
            "admin_audit_action_ck",
        ),
    ] {
        let error = insert(actor, who, action).await.unwrap_err();
        assert_eq!(
            constraint(&error).as_deref(),
            Some(name),
            "{actor} {action}"
        );
    }
}
