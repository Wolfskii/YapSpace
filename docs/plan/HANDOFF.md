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

## 2026-10-26 · P1-09 · TODO → DONE
- Did: `Dockerfile` (cargo-chef, rust:1.98-slim-trixie builder, gcr.io/distroless/cc-debian13:nonroot runtime, HEALTHCHECK = `yapspace healthcheck`), `.dockerignore`.
- Checks: `docker build` ok; image 61.4 MB (target was < 60 MB: accepted deviation); user nonroot; `--help` ok; container /readyz 200 against the dev DB; in-container `healthcheck` exit 0 (Git Bash needs `MSYS_NO_PATHCONV=1` for `docker exec` with /app paths).
- Next: P1-10 (CI) is written. It needs a GitHub remote to run (D-03). P1-11 waits on P1-06 review.

## 2026-10-26 · P1-10 · TODO → IN_REVIEW
- Did: `.github/workflows/ci.yml` (rust: fmt, clippy, migrate from zero, tests with a postgres 18 service; deny; docker: build + compose smoke + Trivy; guards: no {@html}, no AI attribution in PR commits), `compose.ci.yaml`, `.github/dependabot.yml`.
- Checks: YAML parses. Not run on GitHub yet. Deferred on purpose: the OpenAPI drift check (added with P1-11).

## 2026-10-26 · P1-06 · IN_REVIEW → DONE
- Checks: manual Ctrl-C with `task api:run` logged "shutdown signal received" then "shutdown complete" at once.
- Next: P1-11 (OpenAPI skeleton) is unblocked. P1-10 still waits on the GitHub remote (D-03).

## 2026-10-26 · P1-11 · TODO → IN_REVIEW
- Did: `api/openapi.rs` (ApiDoc metadata, `session_token` bearer scheme, `document()` built from the router so `routes!` paths are included), `#[utoipa::path]` on healthz/readyz, `openapi --out` command, `/openapi.json` and `/docs` (Scalar) mounted only when YAPSPACE_ENV=dev, CI drift step, `docs/api/openapi.json` committed.
- Checks: spec contains /healthz and /readyz; regenerating gives byte-identical output; dev: /openapi.json 200 and /docs 200; prod: both 404; tests and clippy pass.
- Not done: Redocly lint. The command was denied by the permission checker (it runs an external npm package). Run `npx @redocly/cli lint docs/api/openapi.json` yourself and set P1-11 to DONE if it passes.
- Next: P1-10 (needs the GitHub remote, D-03).

## 2026-10-26 · P1-10 · IN_REVIEW → DONE
- Did: remote set to https://github.com/Wolfskii/YapSpace.git. Repo kept PUBLIC by user choice. Fixed two CI errors: trivy-action tag needed a v prefix, and the release it pinned depended on a missing setup-trivy tag. Now pinned to aquasecurity/trivy-action@v0.36.0.
- Checks: GitHub CI green on all four jobs (rust, cargo-deny, guards, docker smoke test + Trivy).
- Still open: P1-11 (Redocly lint, run by the user). Phase 1 gate waits on it.


## 2026-10-27 · Session summary (Phases 0 to 5 server side, Phase 4 extension)
- Done this session: identity rules and 90 vectors (P0-04, P2-08..11); auth, discussions, messages, rate limits, spam checks, admin CLI, authz matrix (P2-01..22); realtime socket with fan-out, resume, gap fill and caps (P3-01..08); browser extension with panel, realtime client, tests and Chrome and Firefox builds (P4-01, 03, 04, 06, 08, 09); reports, moderation, blocks, account deletion and export, retention jobs (P5-02, 03, 05, 06, 07); ADRs (P0-03); release workflow (P5-15).
- Gates: `task check` green (fmt, clippy, about 150 tests, cargo-deny, extension type check, extension tests, both builds). GitHub CI green on the Phase 1 commit before the Phase 2 to 5 work. Re-run CI after pushing.
- Gate waiver: the Phase 1 and Phase 2 gates were passed by the user's instruction to keep going, while P1-11 (Redocly lint) is still not run. Run the lint and mark P1-11 DONE.
- Deviations: the dev database is on port 5433 because another local project owns 5432. Migration 0002 was applied with sqlx-cli because the crate cannot compile before its tables exist. The image is 61.4 MB. `sqlx::test` tests need `DATABASE_URL`.
- Not built: P3-09 (k6), P4-12 and P4-13 (manual checklist, Playwright), extension UI for reports, blocks and account settings (P4-10, P5-04, P5-05 and P5-06 client parts), P5-09 to P5-13 (need a VPS, domain and backup provider), P5-14 (legal review), P5-16 (sign-off). Phase 6 and 7 are deferred until production data and a product decision exist.
- Known limitations: TCP reset hides the 1009 and 4004 close codes from the client in tests (the server does close), the slow-consumer 4008 path has no end-to-end test, and rate limits are per process.
- Next: run `npx @redocly/cli lint docs/api/openapi.json`; check the extension in Chrome with `task api:run` and `task ext:dev`; then decide D-01 and D-02 to unblock the deployment items.
