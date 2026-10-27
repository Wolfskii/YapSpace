//! Admin CLI functions: role changes, session revocation, and the dev seed.

mod common;

use sqlx::PgPool;
use yapspace_server::{admin, domain::passwords::PasswordHasher};

const PASSWORD: &str = "correct horse battery";

#[sqlx::test]
async fn set_role_changes_the_role_and_writes_an_audit_event(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    common::register(&app, "alice_01", PASSWORD).await;

    admin::set_role(&pool, "Alice_01", "moderator").await.unwrap();

    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE username = 'alice_01'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(role, "moderator");
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action = 'user.role_changed' AND actor_id IS NULL AND metadata->>'via' = 'cli'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audited, 1);
}

#[sqlx::test]
async fn set_role_for_an_unknown_user_is_an_error(pool: PgPool) {
    assert!(admin::set_role(&pool, "nobody_here", "admin").await.is_err());
}

#[sqlx::test]
async fn revoke_all_sessions_for_one_user_leaves_others_alone(pool: PgPool) {
    let app = common::spawn_app(pool.clone()).await;
    common::register(&app, "alice_01", PASSWORD).await;
    let (bob, _, _) = common::register(&app, "bob_0002", PASSWORD).await;

    let revoked = admin::revoke_all_sessions(&pool, Some("alice_01")).await.unwrap();
    assert_eq!(revoked, 1);
    let live: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE revoked_at IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(live, 1, "bob's session is still live");
    assert!(bob.starts_with("ys_"));
}

#[sqlx::test]
async fn seed_dev_creates_demo_data_and_refuses_in_prod(pool: PgPool) {
    let hasher = PasswordHasher::new();
    let dev = common::test_config();
    admin::seed_dev(&pool, &dev, &hasher).await.unwrap();

    let messages: i64 = sqlx::query_scalar("SELECT count(*) FROM messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(messages, 90);
    let discussions: i64 = sqlx::query_scalar("SELECT count(*) FROM discussions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(discussions, 3);

    let mut prod = common::test_config();
    prod.env = yapspace_server::config::Env::Prod;
    assert!(admin::seed_dev(&pool, &prod, &hasher).await.is_err());
}
