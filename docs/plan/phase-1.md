# Phase 1 · Project foundation

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** P0-01. **Parallelizable:** P1-09/P1-10 after P1-06. P1-11 after P1-07.
**Exit criteria:** `task dev:db:up && task db:migrate && task api:run` serves `/healthz` + `/readyz`, `task check` is green, the Docker image builds
and its healthcheck passes, and `ci.yml` exists (green once the remote exists).

### P1-01 · Repository scaffolding
**Priority:** Must · **Deps:** P0-01 · **Spec:** §N.1, §N.3
**Deliverables:** check that P0-01's `.gitignore`/`.gitattributes`/`.editorconfig`/`AGENTS.md`/`CLAUDE.md` exist. Add `.env.example`:
```dotenv
YAPSPACE_ENV=dev
YAPSPACE_DATABASE_URL=postgres://yapspace:yapspace@127.0.0.1:5432/yapspace
DATABASE_URL=postgres://yapspace:yapspace@127.0.0.1:5432/yapspace   # used by sqlx macros/cli
YAPSPACE_BIND_ADDR=127.0.0.1:8080
YAPSPACE_METRICS_ADDR=127.0.0.1:9090
RUST_LOG=info,yapspace_server=debug,tower_http=info
```
**Steps:** - [ ] create the file · - [ ] `cp .env.example .env` locally (`.env` is gitignored) · - [ ] commit `chore: added dev env example`
**Acceptance:** `git check-ignore .env` prints `.env`.

### P1-02 · Rust workspace skeleton
**Priority:** Must · **Deps:** P1-01 · **Spec:** §C.1, §N.2
**Deliverables:** root `Cargo.toml` (`[workspace] members = ["crates/*"]`, `resolver = "3"`, `[workspace.package] edition = "2024",
version = "0.1.0"`, `[workspace.lints.rust] unsafe_code = "forbid"`, `[workspace.lints.clippy] all = "warn"`), `rust-toolchain.toml`,
`rustfmt.toml`, `deny.toml` (`cargo deny init`, then edit licenses per §N.2), `crates/yapspace-identity/` (`lib.rs` with
`pub fn version() -> u16 { 1 }`), `crates/yapspace-server/` (`[[bin]] name = "yapspace"`, `main.rs` with the clap CLI: subcommands
`serve`, `migrate`, `openapi --out <path>`, `healthcheck`, `admin set-role <username> <user|moderator|admin>`). Each subcommand prints
"not implemented" except `--help`.
**Steps:**
- [ ] 1. Create the files. Each crate gets `[lints] workspace = true`.
- [ ] 2. `cargo add` the server deps from §N.2 (all of them now, so later tasks don't churn `Cargo.lock`; unused-crate warnings don't occur for deps).
- [ ] 3. Create the module files from §C.1 as empty modules with `//!` doc comments describing their responsibility (one line each).
- [ ] 4. `cargo build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`.
**Acceptance:** `cargo run -p yapspace-server -- --help` lists the 5 subcommands. Clippy is clean.
**Notes:** commit `Cargo.lock`.

### P1-03 · Dev Postgres + Taskfile
**Priority:** Must · **Deps:** P1-02 · **Spec:** §N.4, §J.3 (Postgres settings)
**Deliverables:** `compose.yaml` (dev):
```yaml
name: yapspace-dev
services:
  postgres:
    image: postgres:18-alpine
    command: ["postgres", "-c", "shared_preload_libraries=pg_stat_statements", "-c", "log_min_duration_statement=200ms"]
    environment: { POSTGRES_USER: yapspace, POSTGRES_PASSWORD: yapspace, POSTGRES_DB: yapspace }
    ports: ["127.0.0.1:5432:5432"]
    volumes: ["pgdata:/var/lib/postgresql"]
    healthcheck: { test: ["CMD-SHELL", "pg_isready -U yapspace -d yapspace"], interval: 2s, timeout: 3s, retries: 30 }
volumes: { pgdata: {} }
```
and `Taskfile.yml` with the tasks from §N.4 that already make sense (setup, dev:db:*, db:migrate, db:prepare, api:*, docker:build, check).
**Steps:** - [ ] write the files · - [ ] `task dev:db:up` then `task dev:db:psql` → `select version();` shows 18.x · - [ ] commit
**Acceptance:** `task --list` shows the tasks. Postgres becomes healthy within 30 s.
**Notes:** on Windows, port 5432 may already be taken by a local Postgres. If so, use `127.0.0.1:5433:5432` and update `.env.example`.

### P1-04 · Configuration module
**Priority:** Must · **Deps:** P1-02 · **Spec:** §J.5
**Deliverables:** `config.rs`: `pub struct Config { env: Env, database_url: String, db_max_connections: u32, bind_addr: SocketAddr,
metrics_addr: SocketAddr, log_format: LogFormat, trusted_proxies: Vec<IpNet>, allowed_ws_origins: Vec<String>, ws_max_connections: usize,
registration_open: bool, extra_blocked_hosts: Vec<String>, shutdown_grace: Duration }` + `Config::from_env() -> Result<Config, ConfigError>`
and `Config::from_lookup(impl Fn(&str) -> Option<String>)` (the testable core). `*_FILE` support: when `X_FILE` is set, read it and trim it, and it overrides `X`.
**Tests:** unit tests for the defaults, a missing DB URL → error naming the var, `_FILE` precedence, a bad CIDR → error, comma lists with spaces.
**Acceptance:** `yapspace serve` with no DB URL exits with code 2 and prints `missing YAPSPACE_DATABASE_URL (or YAPSPACE_DATABASE_URL_FILE)`.

### P1-05 · Telemetry: logging + metrics
**Priority:** Must · **Deps:** P1-04 · **Spec:** §K
**Deliverables:** `telemetry.rs`: `init_tracing(&Config)` (EnvFilter from `RUST_LOG`, the default `info`. JSON or pretty), `init_metrics(addr) ->
PrometheusHandle` (use `PrometheusBuilder::new().with_http_listener(addr).install()`, or install a recorder and serve `/metrics` on a second
axum listener; pick whichever the installed version's docs show), `http_metrics` middleware (`axum::middleware::from_fn`) recording
`http_requests_total` and `http_request_duration_seconds` labeled with `MatchedPath` (fall back to `"unmatched"`, so raw paths never become labels),
`build_info` gauge.
**Tests:** unit test: the middleware uses the route template label (request `/v1/messages/123` → label `/v1/messages/{id}`).
**Acceptance:** with `api:run` running, `curl 127.0.0.1:9090/metrics` shows `build_info` and the HTTP metrics after one request.

### P1-06 · App skeleton: state, router, errors, health, shutdown
**Priority:** Must · **Deps:** P1-05 · **Spec:** §C.1, §F.1, §G.3 (shutdown)
**Deliverables:**
- `error.rs`: `#[derive(thiserror::Error)] pub enum AppError { Validation(Vec<FieldError>), Unauthorized, InvalidCredentials, Forbidden,
  AccountSuspended, DiscussionLocked, NotFound, UsernameTaken, Conflict(&'static str), UrlUnsupported(&'static str), InvalidParent,
  RateLimited { retry_after_s: u64 }, PayloadTooLarge, BadRequest(String), Unavailable, Internal(anyhow::Error) }` and
  `impl IntoResponse` producing problem+json per §F.1 (`content-type: application/problem+json`, the `code` field, `request_id` read from the
  request extension or the `x-request-id` header copied in by the middleware). `impl From<sqlx::Error>` → `Internal`, except `RowNotFound` → `NotFound`.
  `Internal` logs `error!(error = ?e, request_id)` and returns a generic detail.
- JSON rejection handling: a custom extractor `ApiJson<T>` (wraps `axum::Json`, maps `JsonRejection` → `BadRequest` or 413) that's used everywhere.
- `app.rs`: `#[derive(Clone)] pub struct AppState { pub db: PgPool, pub config: Arc<Config>, pub shutdown: CancellationToken,
  pub tasks: TaskTracker }` (later tasks add `hub`, `limits`, `bus`). `build_router(state) -> Router` with layers, from outer to inner:
  `CatchPanicLayer` → `SetRequestIdLayer::x_request_id(MakeRequestUuid)` → `PropagateRequestIdLayer` → `TraceLayer` (span with request_id,
  method, matched path) → metrics middleware → `CompressionLayer` → `TimeoutLayer(10 s)` **(applied only to the REST sub-router, not
  `/v1/realtime`)** → `RequestBodyLimitLayer(16 KiB)`.
- `api/health.rs`: `/healthz` (always 200 unless shutting down → 503) and `/readyz` (`SELECT 1` with a `tokio::time::timeout` of 2 s).
- `serve()`: bind `TcpListener`, `axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
  .with_graceful_shutdown(shutdown_signal(token))` where `shutdown_signal` waits for ctrl_c or SIGTERM (`tokio::signal::unix` behind `#[cfg(unix)]`),
  then cancels the token. After serve returns: `tasks.close(); timeout(grace, tasks.wait())`, then `pool.close().await`.
- `healthcheck` subcommand: a raw HTTP GET with `tokio::net::TcpStream` to `127.0.0.1:<port from YAPSPACE_BIND_ADDR>/healthz` (no reqwest in the
  release binary). Exit 0 on `HTTP/1.1 200`, otherwise 1.
**Tests:** unit tests for the problem+json shape of 3 variants. An integration test comes in P1-08.
**Acceptance:** `curl -i localhost:8080/healthz` → 200 + `x-request-id`. Unknown route → 404 problem+json. With the DB stopped, `/readyz` → 503.
Ctrl-C exits within 1 s with the log line `shutdown complete`.

### P1-07 · Database pool + migrations + initial schema
**Priority:** Must · **Deps:** P1-03, P1-06 · **Spec:** §D.2, §J.5
**Deliverables:** `db/mod.rs`: `connect(&Config) -> PgPool` using `PgPoolOptions::new().max_connections(cfg.db_max_connections)
.acquire_timeout(3s).connect_with(PgConnectOptions::from_str(url)?.options([("statement_timeout", "5000")]).application_name("yapspace"))`.
`migrations/0001_init.sql` = §D.2 verbatim. `migrate` subcommand: `sqlx::migrate!("./migrations").run(&pool)`. Pool gauges updated every 5 s by
a tiny task (`pool.size()`, `pool.num_idle()`). `.sqlx/` created by `task db:prepare` (empty until queries exist).
**Steps:** - [ ] pool · - [ ] migration file · - [ ] `task db:migrate` · - [ ] `\d messages` in psql shows the constraints · - [ ] `serve` does NOT run
migrations (only the `migrate` subcommand does) · - [ ] commit
**Tests:** `#[sqlx::test]` that inserts a user, a discussion and two messages, and checks that (a) a duplicate `(discussion_id, seq)` fails, (b) a parent in another
discussion fails with a FK violation, (c) a duplicate `(author_id, client_msg_id)` fails, (d) a username `Bad-Name` fails the CHECK.
**Acceptance:** a fresh DB migrates. Running it again is a no-op. The 4 constraint tests pass.

### P1-08 · Integration test harness
**Priority:** Must · **Deps:** P1-07 · **Spec:** §M.1
**Deliverables:** `crates/yapspace-server/src/lib.rs` (move the modules to the lib, so `main.rs` is thin and tests can `use yapspace_server::…`),
`tests/common/mod.rs`: `pub async fn spawn_app(pool: PgPool) -> TestApp` (builds a test `Config`, binds `127.0.0.1:0`, spawns serve on a
task, returns `TestApp { addr, http: reqwest::Client, base: String }` with helpers `get/post_json/delete(path, token: Option<&str>)`,
`register(username) -> (token, user)` (added in P2-04), and `ws_connect() -> WsClient` (added in P3-03)). `tests/health.rs` checks that `/healthz` and `/readyz` are 200.
**Acceptance:** `cargo test --workspace` passes with the dev DB running. The README documents `DATABASE_URL` for `#[sqlx::test]`.

### P1-09 · Dockerfile + image healthcheck
**Priority:** Must · **Deps:** P1-07 · **Spec:** §J.2
**Deliverables:** `Dockerfile`, `.dockerignore`, and the `docker:build` task.
**Steps:** - [ ] write the files · - [ ] `task docker:build` · - [ ] `docker run --rm yapspace:dev --help` · - [ ] `docker run --rm --network host -e
YAPSPACE_DATABASE_URL=… yapspace:dev serve`, then `docker exec <id> /app/yapspace healthcheck` → exit 0 (on Windows Docker Desktop use
`host.docker.internal` in the DB URL instead of host networking) · - [ ] `docker image ls yapspace:dev` → size < 60 MB.
**Acceptance:** the image runs as non-root (`docker inspect` → User `nonroot`). The image is under 60 MB. The healthcheck works.

### P1-10 · CI workflow
**Priority:** Must · **Deps:** P1-08, P1-09 · **Spec:** §M.3
**Deliverables:** `.github/workflows/ci.yml` with the jobs `rust`, `deny`, `docker`, `guards` (`extension` is added in P4-01), plus `.github/dependabot.yml`
(cargo weekly, npm weekly for `/extension`, github-actions monthly, docker monthly), and `compose.ci.yaml`.
**Steps:** - [ ] write the workflows · - [ ] validate locally with `npx -y @action-validator/cli .github/workflows/ci.yml` (or `actionlint` if installed)
· - [ ] if D-03 (remote) is decided: push and see green, otherwise set the task to `IN_REVIEW` with the note "needs GitHub remote".
**Acceptance:** the workflow file validates. When pushed, all jobs pass.

### P1-11 · OpenAPI skeleton + docs UI
**Priority:** Must · **Deps:** P1-06 · **Spec:** §F.4
**Deliverables:** `api/openapi.rs` (`#[derive(OpenApi)]` with title "YapSpace API", version from `CARGO_PKG_VERSION`, a bearer security scheme
`session_token`), with the routers built through `OpenApiRouter`. The `openapi --out` subcommand writes pretty JSON. `/docs` (Scalar) and `/openapi.json` are only mounted
when `env == dev`. `docs/api/openapi.json` is committed. The CI drift step is added to `ci.yml`.
**Acceptance:** `task api:openapi` produces valid OpenAPI 3.1 with the health paths. Running it a second time gives no git diff.
