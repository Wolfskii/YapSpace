# Phase 2 · Backend MVP (REST)

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** the Phase 1 gate has PASSED. **Parallelizable:** the identity crate (P2-08..P2-11) can run in parallel with auth (P2-01..P2-07).
**Exit criteria:** every endpoint in §F.2 not marked Phase 5 is implemented, documented in OpenAPI and covered by integration tests, the authz matrix is green,
the seq concurrency test is green, and `task check` is green.

### P2-01 · Validation helpers
**Priority:** Must · **Deps:** P1-08 · **Spec:** §F.3
**Deliverables:** `domain/validation.rs`: `normalize_username(&str) -> Result<String, FieldError>`, `validate_password(pw, username)`,
`normalize_body(&str) -> Result<String, FieldError>` (NFC → reject control chars except `\n`/`\t` → trim → collapse `\n{3,}` → `\n\n` →
count chars 1..=2000), `sanitize_title(&str) -> Option<String>`, `truncate_label`. A `FieldError { field, code, message }` that's serializable.
**Tests:** ≥ 25 unit cases (reserved names, `deleted_x`, uppercase input lowercased, 2000 vs 2001 chars with multibyte emoji, `\u{0007}` rejected,
CRLF handling (`\r\n` → `\n` first), whitespace-only body rejected).

### P2-02 · Password hashing & tokens
**Priority:** Must · **Deps:** P2-01 · **Spec:** §H.1
**Deliverables:** `domain/passwords.rs`: `PasswordHasher { sem: Arc<Semaphore> }` with `async fn hash(&self, pw) -> Result<String>`,
`async fn verify(&self, pw, phc) -> Result<bool>` (both `spawn_blocking`), `async fn verify_dummy(&self, pw)` (runs against a constant PHC
generated once at startup via `OnceLock`), `needs_rehash(phc) -> bool`. `domain/tokens.rs`: `new_session_token() -> (String /*ys_…*/,
[u8; 32] /*sha256*/)`, `hash_token(&str) -> [u8;32]`, `new_recovery_code() -> (String /*XXXXX-XXXXX-XXXXX-XXXXX*/, [u8;32])`,
`normalize_recovery_code(&str) -> String`. The Crockford alphabet is `0123456789ABCDEFGHJKMNPQRSTVWXYZ`.
**Tests:** hash→verify true, wrong pw false, the PHC string starts with `$argon2id$v=19$m=19456,t=2,p=1$`, the token has the prefix and length 46, recovery normalization
(lowercase, spaces, missing dashes), no two tokens are equal over 1000 generations.

### P2-03 · Sessions + auth extractors
**Priority:** Must · **Deps:** P2-02 · **Spec:** §D.3 Q6, §H.1
**Deliverables:** `db/sessions.rs` (create, find_by_token_hash (Q6), touch (sliding update), revoke, revoke_all_for_user(except: Option<Uuid>),
list_for_user), `db/users.rs` (insert, find_by_username, find_by_id, update_password…), `api/extractors.rs`:
`AuthUser { user_id, session_id, username, role, status }` implementing `FromRequestParts<AppState>`: parse `Authorization: Bearer ys_…`
(missing or malformed → `Unauthorized`), run Q6, call touch (fire and forget: `tokio::spawn`, errors logged), treat a suspension past
`suspended_until` as active. `MaybeAuthUser(Option<AuthUser>)` (an invalid token → `Unauthorized`, not anonymous, so clients notice).
`ModUser`/`AdminUser` wrappers (role check → `Forbidden`). `fn ensure_can_write(&AuthUser) -> Result<(), AppError>` (suspended → `AccountSuspended`).
**Tests:** an expired session → 401, a revoked session → 401, touch updates at most every 10 min (call twice, check `last_used_at` didn't change on the 2nd).

### P2-04 · Register
**Priority:** Must · **Deps:** P2-03 · **Spec:** §F.2, §H.1, §H.4
**Deliverables:** `POST /v1/auth/register`. Flow: config.registration_open else 403 → validate → hash pw → **one tx**: insert user (unique violation
on `users_username_key` → `UsernameTaken`), insert password_credentials with the recovery hash, insert session → commit → 201 `{user, session_token,
recovery_code}`. Records an `audit_events` row `user.registered`. (The rate limit is wired in P2-18.)
**Tests:** success shape, duplicate username (different case) → 409 `username_taken`, invalid username → 422 with `errors[0].field == "username"`,
password == username → 422, unknown field → 400. The `TestApp::register` helper is added.

### P2-05 · Login / logout
**Priority:** Must · **Deps:** P2-04
**Deliverables:** `POST /v1/auth/login` (unknown user → `verify_dummy` + `InvalidCredentials`; deleted user → same; wrong pw → same; rehash if
needed; a suspended user CAN log in (they can read and see the suspension notice)), `POST /v1/auth/logout` (revoke the current session → 204).
**Tests:** login ok, wrong pw 401, unknown user 401 with an identical body (compare JSON minus request_id), logout then using the token → 401.

### P2-06 · Me endpoints
**Priority:** Must · **Deps:** P2-05
**Deliverables:** `GET /v1/me`, `POST /v1/me/password` (verify current → hash new → update → revoke the other sessions → audit `user.password_changed`),
`POST /v1/me/recovery-code`, `GET /v1/me/sessions` (with `current: bool`), `DELETE /v1/me/sessions/{id}` (only your own, otherwise 404),
`GET /v1/users/{username}`.
**Tests:** a password change invalidates the other session but not the current one. Deleting another user's session id → 404.

### P2-07 · Account recovery
**Priority:** Must · **Deps:** P2-06
**Deliverables:** `POST /v1/auth/recover`: find the user → compare `sha256(normalize(code))` with the stored hash in **constant time** (`subtle` crate or
`ring`-free manual xor fold. Add `subtle@2`) → set the new pw + a new recovery code → revoke all sessions → create a new session → audit `user.recovered`.
A wrong username or code → `InvalidCredentials` (and a dummy verify for timing).
**Tests:** success rotates the code (the old code then fails), old sessions → 401, a wrong code → 401.

### P2-08 · Identity crate: generic normalization
**Priority:** Must · **Deps:** P0-04, P1-02 · **Spec:** §E.2, §E.3, §E.6
**Deliverables:** `yapspace-identity/src/{lib.rs, error.rs, rules_v1.rs, normalize.rs}`. Public API:
```rust
pub const RULESET: u16 = 1;
pub enum RejectReason { Invalid, TooLong, UnsupportedScheme, IpHost, PrivateHost, BlockedHost, SensitiveParameters, SecretLikeToken }
pub struct Normalized { pub host: String /* with :port */, pub value: String, pub display_url: String, pub path: String }
pub fn normalize(input: &str, extra_blocked_hosts: &[String]) -> Result<Normalized, RejectReason>;
pub fn registrable_domain(host: &str) -> Option<String>;   // psl
```
`RejectReason::as_str()` returns the §F.1 snake_case strings.
**Tests:** `tests/vectors.rs` (all vectors without a canonical or adapter expectation. Mark the adapter vectors as skipped until P2-10 via `if v.expects_adapter() { continue }`),
plus proptest for idempotence and no panics.

### P2-09 · Identity crate: privacy rules completeness
**Priority:** Must · **Deps:** P2-08 · **Spec:** §E.3 6a/6f/7b/7c, §E.6
**Deliverables:** the secret-like detector, sensitive prefixes, `;` path params, blocked host suffix matching (`.dropbox.com` matches `www.dropbox.com`
and `dropbox.com`), and the extra blocked hosts from config.
**Tests:** every rejection vector passes. A unit test per list entry family.

### P2-10 · Identity crate: site adapters
**Priority:** Must · **Deps:** P2-08 · **Spec:** §E.4
**Deliverables:** `adapters/{mod.rs, youtube.rs, x.rs, reddit.rs, hn.rs}` implementing `SiteAdapter`, plus `pub fn adapter_key(url: &Url) -> Option<AdapterKey { adapter, value, display_url }>`.
**Tests:** all adapter vectors pass. Non-matching paths fall through to generic.

### P2-11 · Identity crate: resolution candidates
**Priority:** Must · **Deps:** P2-09, P2-10 · **Spec:** §E.5
**Deliverables:** `pub struct Candidate { pub key: String, pub key_hash: [u8;32], pub kind: Kind /* Url|Canonical|Adapter */, pub display_url: String, pub site: String }`
and `pub fn candidates(observed: &str, canonical: Option<&str>, extra_blocked: &[String]) -> Result<Vec<Candidate>, RejectReason>` (never empty on Ok).
`site` = the normalized host without the port.
**Tests:** **all** vectors pass now (remove the skip). Canonical acceptance/rejection cases.

### P2-12 · Resolve endpoint
**Priority:** Must · **Deps:** P2-11, P2-03 · **Spec:** §F.2, §D.3 Q1, §E.5
**Deliverables:** `db/discussions.rs::find_by_key_hash`, `domain/discussions.rs::resolve(pool, url, canonical) -> (Candidate /*primary*/, Option<Discussion>)`
(loop over the candidates, first hit wins, follow `merged_into_id` one hop, a `hidden` discussion → treat it as nonexistent for non-mods),
`POST /v1/discussions/resolve` → `{identity, discussion}`. A rejection → 422 `url_unsupported` + `reason`. **Never inserts.**
**Tests:** an unknown URL → discussion null. After a create (P2-13), the same URL with a utm param resolves to it. A blocked host → 422 with the reason. The URL
does not appear in logs (capture the logs with `tracing-test`, or review the code: no `url` field in spans).

### P2-13 · Create discussion
**Priority:** Must · **Deps:** P2-12 · **Spec:** §D.3 Q4
**Deliverables:** `POST /v1/discussions` `{url, canonical_url?, title?}` → resolve; if found → 200 existing; else tx Q4 → 201; on conflict →
re-run the lookup → 200. Requires `AuthUser` + `ensure_can_write`. Records `created_by`.
**Tests:** create → 201, the same again → 200 with the same id, **20 concurrent creates → exactly one row in `discussions`**, and a title is sanitized.

### P2-14 · Get discussion
**Priority:** Must · **Deps:** P2-13 · **Deliverables:** `GET /v1/discussions/{id}` (follows a merge, sets `merged_from`). Hidden → 404 for non-mods.

### P2-15 · Post message
**Priority:** Must · **Deps:** P2-14 · **Spec:** §D.3 Q5, §F
**Deliverables:** `domain/messages.rs::post(state, user, discussion_id, body, parent_id, client_msg_id) -> (Message, created: bool)`, implemented
exactly as Q5. A unique violation on `messages_author_client_key` → rollback and return the existing message (200). The parent missing or in another discussion, or
the parent status != visible → `InvalidParent`. A locked discussion → `DiscussionLocked`. A missing or hidden discussion → `NotFound`. The DTO mapper
`MessageDto::from_row` lives in `api/messages.rs` and hides the body unless visible and the author if deleted. Metric `messages_created_total`.
(Publishing to realtime is added in P3-05. Leave a `// P3-05: publish here after commit` marker.)
**Tests:** 201 then a retry → 200 with the same id and seq. A reply with a parent from another discussion → 422 `invalid_parent`. **50 concurrent posts → seqs exactly
1..=50** (`SELECT array_agg(seq ORDER BY seq)`). A body that is too long → 422. Locked → 403 `discussion_locked`.

### P2-16 · List messages (pagination)
**Priority:** Must · **Deps:** P2-15 · **Spec:** §D.3 Q2/Q3, §F.1
**Deliverables:** `GET /v1/discussions/{id}/messages?before_seq&after_seq&limit` (both cursors given → 400. limit clamped 1..=100, default 50).
Fetch limit+1 to set `has_more`. Always return ascending. Set `next_before_seq` = the smallest seq returned when it's a before/newest query with
has_more, and `next_after_seq` = the largest seq returned when it's an after query with has_more.
**Tests:** 120 messages: newest page = seqs 71..120 and has_more. Following the cursor → 21..70, then 1..20 with has_more=false. `after_seq=100&limit=10` →
101..110. Tombstones appear with body null. **Run `EXPLAIN` on Q2 in a test and assert the plan contains `messages_discussion_seq_key`** (to guard the index).

### P2-17 · Single message + author delete
**Priority:** Must · **Deps:** P2-16
**Deliverables:** `GET /v1/messages/{id}` (a hidden discussion → 404), `DELETE /v1/messages/{id}` (author only, otherwise 404; sets `status='deleted'`,
`body=''`, `status_changed_at`; idempotent 204). Leave a `// P3-05` publish marker.
**Tests:** another user deleting → 404. After a delete, the GET shows a tombstone. A reply to a deleted parent → 422.

### P2-18 · Rate limiting + client IP
**Priority:** Must · **Deps:** P2-17 · **Spec:** §H.4, §J.4
**Deliverables:** `client_ip.rs` (`ClientIp` extractor: the peer from `ConnectInfo`; if the peer is in trusted_proxies, the right-most valid IP in `X-Forwarded-For`;
IPv6 → key the /64 prefix). `ratelimit.rs`: `pub struct Limits { register: KeyedLimiter<IpKey>, login_ip, login_ip_user, recover, resolve,
create_discussion: KeyedLimiter<Uuid>, post_user, post_user_new, post_ip, report, global_ip }` with `fn check(&self, bucket, key) -> Result<(), AppError::RateLimited>`
(compute retry_after from governor's `NotUntil::wait_time_from(clock.now())`, rounded up and at least 1 s). Global per-IP is a middleware layer.
Wire each bucket into the handlers. The `jobs.rs` cleanup task calls `retain_recent()` every 60 s. Metric `ratelimit_rejections_total{bucket}`.
Config allows a test override (`Limits::unlimited()` for most tests, `Limits::from_config` normally).
**Tests:** a 6th registration from the same IP → 429 with `Retry-After`. The XFF trust test: XFF from an untrusted peer is ignored. An account < 24 h old gets the stricter post limit.

### P2-19 · Duplicate-message guard
**Priority:** Should · **Deps:** P2-18 · **Spec:** §H.4
**Deliverables:** a `moka::sync::Cache<(Uuid, [u8;32]), ()>` with a 10 min TTL and max capacity 200k in AppState. Checked in `post` **before** the tx (a fast-fail)
and inserted after commit. The new-account multi-discussion check uses a `moka` cache of `(user_id) → small vec of (discussion_id, Instant)`.
**Tests:** the same body twice in different discussions → 2nd gets 422 `duplicate_message`. Different bodies are fine.

### P2-20 · Admin CLI
**Priority:** Must · **Deps:** P2-06
**Deliverables:** `yapspace admin set-role <username> <role>` (an audit event with actor null, metadata `{"via":"cli"}`), `yapspace admin
revoke-all-sessions [--user <username>]`, `yapspace admin seed-dev` (refuses in prod: creates users alice/bob/mod (pw `password123!`) and 3
discussions with 30 messages each, for manual testing).
**Tests:** an integration test calls the functions behind the CLI (not the process).

### P2-21 · OpenAPI completeness + API doc
**Priority:** Must · **Deps:** P2-17 · **Spec:** §F
**Deliverables:** every handler has `#[utoipa::path]` with the request body, the responses (including a `ProblemDetails` schema for the errors), the security scheme,
and examples for the register/post/list payloads. Regenerate `docs/api/openapi.json`. `docs/architecture/api.md` links to it.
**Acceptance:** `npx -y @redocly/cli lint docs/api/openapi.json` has no errors. The drift check passes.

### P2-22 · Authorization matrix + hardening tests
**Priority:** Must · **Deps:** P2-21
**Deliverables:** `tests/authz_matrix.rs`: a table-driven test over every route × {anonymous, user, other user, suspended, deleted-session}
asserting the expected status. A body over 16 KiB → 413 problem+json. Malformed JSON → 400. A wrong content-type → 400/415 (document which).
**Acceptance:** the matrix covers every route in the OpenAPI (the test fails if a route is missing from the table: compare against the paths in `openapi.json`).
