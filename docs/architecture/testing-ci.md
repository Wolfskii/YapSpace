# Testing strategy & CI/CD

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## M. Testing strategy & CI/CD

### M.1 Backend tests
- **Unit** (in-module `#[cfg(test)]`): validation, password hashing params, token generation/hash, recovery-code normalization, error mapping,
  protocol serde round-trips, hub subscribe/publish/cleanup, rate-limit key derivation, client IP extraction.
- **Identity:** `yapspace-identity/tests/vectors.rs` runs every vector in `spec/url-identity/vectors.json`, plus property tests
  (`proptest`: normalize is idempotent: `normalize(display_url(normalize(x))) == normalize(x)`; it never panics on arbitrary strings).
- **DB integration:** `#[sqlx::test(migrations = "./migrations")]` gives each test its own fresh database (needs `DATABASE_URL` pointing at the dev
  Postgres; sqlx creates and drops the test DBs).
- **API integration** (`tests/*.rs`): `common::spawn_app(pool) -> TestApp { addr, client }` binds `127.0.0.1:0`, uses reqwest for HTTP and
  tokio-tungstenite for WS. Files: `auth.rs`, `discussions.rs`, `messages.rs`, `pagination.rs`, `authz_matrix.rs`, `ratelimit.rs`,
  `realtime.rs`, `moderation.rs`, `account.rs`.
- **Concurrency:** 50 concurrent posts to one discussion → seq values exactly 1..=50, no gaps or duplicates. Concurrent create of the same discussion
  from 20 tasks → exactly 1 discussion. 2 WS subscribers + 20 concurrent publishers → both receive every seq in order.
- **WS lifecycle:** subscribe/backlog replay, the gap > 500 → resync, a slow consumer (a reader that never reads) gets 4008, heartbeat timeout, too many
  subscriptions, origin rejected, shutdown → 1012.
- **Security:** IDOR (deleting/reading someone else's session → 404), suspended user can't post, mod-only routes return 403 for users, body limit → 413,
  unknown fields → 400, deleted user content is erased.
- **OpenAPI drift:** CI regenerates the spec and runs `git diff --exit-code docs/api/openapi.json`.

### M.2 Extension tests
- **Vitest unit:** `page-url.ts`, `linkify.ts` (javascript: links never become links), `realtime.ts` (with a fake WebSocket: backoff, dedupe,
  resubscribe with after_seq, resync), the discussion state machine, and the message guards.
- **Component:** `@testing-library/svelte` + `jsdom`: MessageItem renders `<img onerror>` as text, Composer counter/limits/enter-to-send,
  AuthForm validation, plus axe checks (`vitest-axe`).
- **Background:** WXT `fakeBrowser`: action click → executeScript is called, failure → badge set, context routing per tab.
- **E2E (Playwright, Chromium):** builds with `--mode e2e` (adds `e2e-autoload` content script), launches a persistent context with
  `--load-extension`, serves fixture pages on `localhost:4173` (normal article, SPA with pushState, page with a stale canonical, page with hostile CSS
  `* { display:none !important }` and a strict CSP), and runs against a real backend from `docker compose -f compose.yaml up` + `yapspace serve`.
  Flows: register → post → second browser context sees the message live → reload → history persists → report.
- **Firefox:** `web-ext lint` in CI plus a manual checklist (`docs/extension/manual-test-checklist.md`) run before each release. Playwright can't load
  Firefox extensions in a reliable way.

### M.3 CI workflows (`.github/workflows/`)
- `ci.yml` (on push to `develop`/`main` and on PRs):
  - `rust`: services postgres:18-alpine. Steps: checkout → `dtolnay/rust-toolchain@1.98` (clippy, rustfmt) → `Swatinem/rust-cache` →
    `cargo fmt --all --check` → `cargo clippy --workspace --all-targets -- -D warnings` → install sqlx-cli → `sqlx migrate run` →
    `cargo sqlx prepare --workspace --check -- --all-targets` → `cargo test --workspace` → OpenAPI drift check.
  - `deny`: `EmbarkStudios/cargo-deny-action`.
  - `extension`: Node 22 → `npm ci` → `npm run lint` (eslint + prettier --check + svelte-check) → `npm test` → `npm run build` + `build:firefox`
    → `npx web-ext lint -s .output/firefox-mv3` → upload the zips as artifacts.
  - `docker`: buildx build (no push) → Trivy scan (fail on CRITICAL/HIGH with a fix available) → compose smoke test: start postgres+migrate+api with
    `compose.ci.yaml`, curl `/readyz`, register + post through curl.
  - `guards`: grep for `{@html` in `extension/src`, `AssertSqlSafe` without `SAFETY`, `Co-Authored-By` in the commit range.
- `nightly.yml` (schedule): `cargo audit`/deny advisories, `npm audit`, Playwright e2e, the k6 smoke (`browse.js` 2 min at 200 VUs against compose).
- `release.yml` (on tag `v*.*.*`): build and push `ghcr.io/<owner>/yapspace:{version, sha}` (multi-arch amd64 [+ arm64 optional]), build the
  extension zips with the version taken from the tag, and create a GitHub Release with the zips + changelog.
- **Deploy is manual** (`task deploy TAG=…` from the developer machine, or `workflow_dispatch` with environment approval later). It's never automatic
  on push. That's the right default for a solo developer: no surprise migrations, and releases are explicit.

### M.4 Migration safety (expand
1. **Expand** (release N): only additive changes, such as new nullable columns or new tables. Build indexes with `CREATE INDEX CONCURRENTLY` in its own migration file
   with `-- no-transaction` (the sqlx directive) once tables are large. The app N writes both the old and new shapes if needed.
2. **Backfill** in batches (a job or a CLI command), not in the migration.
3. **Contract** (release N+1 or later): drop or rename the old columns only after N−1 is no longer deployed anywhere.
4. Every migration PR has: a `migrations/README.md` entry stating whether it's *expand* or *contract*, a test that runs the
   previous release's integration tests against the new schema (nightly, from P5 on), and a plan for when a rollback isn't possible (restore from backup).

### M.5 Infrastructure tests
Docker build (CI), compose smoke (CI), migrations from zero (every CI run) and from the previous release's schema (nightly from P5), the backup → restore test
(weekly on the server + `task ops:restore-test`), the upgrade test (`deploy/upgrade-test.sh`: start the previous tag, seed, switch to the new tag, smoke), Trivy +
cargo-deny + npm audit, and k6 load/soak (manual before a release plus the nightly smoke).
