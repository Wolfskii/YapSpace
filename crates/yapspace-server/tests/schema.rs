//! Schema tests: the constraints in migrations/0001_init.sql hold. Each test gets a fresh database (sqlx::test).
//! Needs DATABASE_URL pointing at a Postgres server (`task dev:db:up`).

use sqlx::PgPool;
use uuid::Uuid;
use yapspace_server::db;

const UNIQUE_VIOLATION: &str = "23505";
const FOREIGN_KEY_VIOLATION: &str = "23503";
const CHECK_VIOLATION: &str = "23514";

async fn seed_user(pool: &PgPool, username: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, username) VALUES ($1, $2)")
        .bind(id)
        .bind(username)
        .execute(pool)
        .await
        .expect("seed user");
    id
}

async fn seed_discussion(pool: &PgPool, created_by: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO discussions (id, display_url, site, created_by) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind("https://example.com/a")
        .bind("example.com")
        .bind(created_by)
        .execute(pool)
        .await
        .expect("seed discussion");
    id
}

async fn insert_message(
    pool: &PgPool,
    discussion: Uuid,
    seq: i64,
    author: Uuid,
    parent: Option<Uuid>,
    client_msg_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO messages (id, discussion_id, seq, author_id, parent_id, body, client_msg_id)
         VALUES ($1, $2, $3, $4, $5, 'hello', $6)",
    )
    .bind(id)
    .bind(discussion)
    .bind(seq)
    .bind(author)
    .bind(parent)
    .bind(client_msg_id)
    .execute(pool)
    .await?;
    Ok(id)
}

fn sqlstate(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db) => db.code().map(|c| c.to_string()),
        _ => None,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_seq_in_one_discussion_is_rejected(pool: PgPool) {
    let alice = seed_user(&pool, "alice").await;
    let discussion = seed_discussion(&pool, alice).await;

    insert_message(&pool, discussion, 1, alice, None, Uuid::now_v7())
        .await
        .expect("first message");
    let err = insert_message(&pool, discussion, 1, alice, None, Uuid::now_v7())
        .await
        .expect_err("same seq must fail");
    assert_eq!(sqlstate(&err).as_deref(), Some(UNIQUE_VIOLATION));
}

#[sqlx::test(migrations = "./migrations")]
async fn reply_to_message_in_another_discussion_is_rejected(pool: PgPool) {
    let alice = seed_user(&pool, "alice").await;
    let first = seed_discussion(&pool, alice).await;
    let second = seed_discussion(&pool, alice).await;
    let parent = insert_message(&pool, first, 1, alice, None, Uuid::now_v7())
        .await
        .expect("parent");

    let err = insert_message(&pool, second, 1, alice, Some(parent), Uuid::now_v7())
        .await
        .expect_err("cross-discussion reply must fail");
    assert_eq!(sqlstate(&err).as_deref(), Some(FOREIGN_KEY_VIOLATION));
}

#[sqlx::test(migrations = "./migrations")]
async fn same_client_message_id_from_one_author_is_rejected(pool: PgPool) {
    let alice = seed_user(&pool, "alice").await;
    let discussion = seed_discussion(&pool, alice).await;
    let client_msg_id = Uuid::now_v7();

    insert_message(&pool, discussion, 1, alice, None, client_msg_id)
        .await
        .expect("first send");
    let err = insert_message(&pool, discussion, 2, alice, None, client_msg_id)
        .await
        .expect_err("retry with the same client id must not insert twice");
    assert_eq!(sqlstate(&err).as_deref(), Some(UNIQUE_VIOLATION));
}

#[sqlx::test(migrations = "./migrations")]
async fn username_with_uppercase_or_dash_is_rejected(pool: PgPool) {
    let err = sqlx::query("INSERT INTO users (id, username) VALUES ($1, $2)")
        .bind(Uuid::now_v7())
        .bind("Bad-Name")
        .execute(&pool)
        .await
        .expect_err("username format must be enforced by the database");
    assert_eq!(sqlstate(&err).as_deref(), Some(CHECK_VIOLATION));
}

#[sqlx::test(migrations = "./migrations")]
async fn running_migrations_again_is_a_no_op(pool: PgPool) {
    db::migrate(&pool).await.expect("first re-run");
    db::migrate(&pool).await.expect("second re-run");
}
