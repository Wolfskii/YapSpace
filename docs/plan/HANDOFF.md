# Handoff log

Newest entry at the bottom. Template:

```markdown
## YYYY-MM-DD · P1-04 · IN_PROGRESS → DONE
- Did: …
- Commits: <hash> <subject>
- Checks: …
- Next: … Gotcha: …
```

## 2026-10-26 · P0-01 · TODO → DONE
- Did: plan split into `docs/architecture/`, `docs/adr/README.md`, `docs/plan/` (board + phase files), `AGENTS.md`, `CLAUDE.md`, `README.md`, `.gitignore`, `.gitattributes`, `.editorconfig`.
- Not yet: ADR files (P0-03) and URL vectors (P0-04) are still TODO.
- Next: P1-01 (or P0-03 / P0-04 in parallel).

## 2026-10-26 · P1-01 · TODO → DONE
- Did: `.env.example` (dev vars, no secrets). Local `.env` copied (gitignored, verified with `git check-ignore`).
- Commits: see `git log --oneline -1`. Message: chore: added dev env example
- Checks: `git check-ignore -v .env` matches `.gitignore`.
- Next: P1-02 (Rust workspace skeleton + CLI).

## 2026-10-26 · P1-02 · TODO → DONE
- Did: root Cargo.toml (workspace, edition 2024, lints), rust-toolchain.toml (1.98), rustfmt.toml, deny.toml; crates/yapspace-identity (lib stub, RULESET=1); crates/yapspace-server (lib with empty module tree per §C.1, bin `yapspace` with clap subcommands serve/migrate/openapi/healthcheck/admin set-role that print 'not implemented yet' and exit 1). Dependencies from §N.2 added, with the crate versions in the plan. Cargo.lock committed.
- Checks: `cargo build --workspace` ok; `cargo fmt --all --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `yapspace --help` lists the 5 subcommands.
- Gap: `cargo deny check` not run yet (cargo-deny not installed). Install in P1-03 (`task setup`) and run it there.
- Next: P1-03 (dev Postgres compose + Taskfile).

## 2026-10-26 · P1-03 · TODO → DONE
- Did: `compose.yaml` (dev Postgres 18-alpine on 127.0.0.1:5432, healthcheck) and `Taskfile.yml` (setup, dev:db:*, db:*, api:*, docker:build, check). `cargo-deny` installed. `deny.toml` fixed: `private = { ignore = true }` for unpublished workspace crates; workspace license field removed (not SPDX); path dep has a version.
- Checks: `task dev:db:up` healthy, Postgres 18.4 confirmed; `task check` exit 0 (lint, tests, deny).
- Notes: `cargo deny` prints duplicate-version warnings (`multiple-versions = warn`). Acceptable for now; revisit at P5-08.
- Next: P1-05 (telemetry).

## 2026-10-26 · P1-04 · TODO → DONE
- Did: `config.rs` (Config::from_env / from_lookup, `_FILE` precedence, lists, bool/number parsing, validation, redacting Debug). 9 unit tests. `serve` loads config and exits 2 with a clear message when the DB URL is missing.
- Checks: `cargo test -p yapspace-server config` 9 passed; direct run of the binary exits 2 without the URL. (Note: running through `env -u` in Git Bash returned a misleading exit 0. Run the binary directly to check exit codes.)
- Next: P1-05 (telemetry: logging + metrics).

## 2026-10-26 · P1-05 · TODO → DONE
- Did: `telemetry.rs`: tracing (JSON or pretty, EnvFilter), Prometheus exporter on `YAPSPACE_METRICS_ADDR`, `http_metrics` middleware (route_layer, so labels are route templates; unmatched routes are not counted), `build_info` gauge.
- Checks: `curl 127.0.0.1:9090/metrics` shows `build_info` and `http_requests_total{route="/healthz",…}`. Unit test: `MatchedPath` is the template.

## 2026-10-26 · P1-06 · TODO → IN_REVIEW
- Did: `error.rs` (AppError → RFC 9457 problem+json, Retry-After, internal errors hide details), `api/extractors.rs` (ApiJson), `app.rs` (AppState, middleware stack, request-id sanitising, `request_id` filled into problem bodies, serve/serve_on, graceful shutdown order, healthcheck client), `api/health.rs`.
- Checks: healthz 200 + x-request-id; unknown route 404 problem+json with matching request_id; readyz 200 with DB, 503 with DB stopped (≈2 s check timeout), healthz stays 200; `yapspace healthcheck` exit 0.
- Not verified: Ctrl-C through the real console. Shutdown itself is covered by the test `shutdown_stops_the_server_within_the_grace_period`. Manual step: run `task api:run`, press Ctrl-C, expect "shutdown signal received" then "shutdown complete" within 1 s. Set DONE after that.
- Decision made here: TraceLayer logs 5xx at WARN, not ERROR (readyz 503 is expected during outages). Spans carry method and request_id only, never the URI.

## 2026-10-26 · P1-07 · TODO → DONE
- Did: `db/mod.rs` (pool with statement_timeout 5 s and application_name, embedded MIGRATOR, pool gauges every 5 s), `migrations/0001_init.sql` (extracted verbatim from docs/architecture/data-model.md §D.2), `migrate` subcommand. `serve` does not migrate.
- Checks: `tests/schema.rs` 5 passed (duplicate seq, cross-discussion reply FK, duplicate client_msg_id, username CHECK, migrations re-run is a no-op). `yapspace migrate` run twice.

## 2026-10-26 · P1-08 · TODO → DONE
- Did: `tests/common/mod.rs` (`spawn_app(pool)` binds 127.0.0.1:0, returns TestApp with http client and `stop()`), `tests/health.rs` (7 tests), `serve_on(state, listener)` split from `serve()` so tests use an ephemeral port without installing signal handlers. README documents DATABASE_URL for sqlx::test.
- Checks: `task check` exit 0 (fmt, clippy -D warnings, 30 tests, cargo-deny).
- Next: P1-09 (Dockerfile). Still open from earlier: the `.sqlx/` offline cache is created in P2 when real queries exist; `cargo install sqlx-cli` must run before `task db:prepare` works.
