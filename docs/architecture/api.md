# API specification

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## F. API specification

### F.1 Conventions
- Base path `/v1`. Breaking changes → `/v2`, running side by side for at least 6 months. Additive changes (new optional fields, new endpoints) don't bump the version.
  Clients **must ignore unknown fields**.
- JSON only, UTF-8, `snake_case` fields. Request bodies are limited to **16 KiB** (tower-http `RequestBodyLimitLayer`). Unknown request fields
  are rejected (`#[serde(deny_unknown_fields)]`) so client bugs surface early.
- Auth: `Authorization: Bearer ys_<43 chars base64url>`. **No cookies**, so CSRF doesn't apply. CORS is disabled: extension pages with
  host permissions aren't subject to CORS, and there is no web client yet.
- Every response carries `x-request-id` (generated, or propagated from the incoming header if it's valid).
- Timestamps are RFC 3339 UTC with milliseconds. IDs are UUID strings. `seq` is a JSON number (i64, safe up to 2^53 in JS).
- **Errors:** RFC 9457 `application/problem+json`:
```json
{"type": "https://yapspace.app/errors/validation_failed", "title": "Validation failed", "status": 422,
 "code": "validation_failed", "detail": "body must be 1-2000 characters", "request_id": "01J…",
 "errors": [{"field": "body", "code": "length", "message": "must be 1-2000 characters"}]}
```
  Error codes (stable; clients switch on `code`): `bad_request 400`, `invalid_credentials 401`, `unauthorized 401`, `forbidden 403`,
  `account_suspended 403`, `discussion_locked 403`, `not_found 404`, `username_taken 409`, `conflict 409`, `payload_too_large 413`,
  `validation_failed 422`, `url_unsupported 422` (+ `"reason"`: invalid|too_long|unsupported_scheme|ip_host|private_host|blocked_host|
  sensitive_parameters|secret_like_token), `invalid_parent 422`, `rate_limited 429` (+ `Retry-After` header and a `"retry_after_s"` field),
  `internal 500`, `unavailable 503`. A 500 never leaks internals: log the error with its request_id and return a generic detail.
- **Pagination (messages):** cursors are `seq` values. Request `before_seq` XOR `after_seq` (or neither, which gives the newest page), and `limit` 1–100
  (default 50). The response is always **ascending by seq**:
  `{"items": [...], "has_more": true, "next_before_seq": 101 | null, "next_after_seq": 150 | null}`. `has_more` refers to the direction requested.
- **Idempotency:** `POST …/messages` requires a client-generated `client_msg_id` (UUID). A retry with the same id returns the original message
  with **200** (first time **201**). `POST /v1/discussions` is naturally idempotent by identity key. `POST /v1/reports` is idempotent by the unique constraint
  and returns the existing report with 200.
- **Rate limits** (§H.4) → `429`. The limiter key and bucket are logged at debug level, without the IP.

### F.2 Endpoints (MVP)
| Method & path | Auth | Purpose | Success |
|---|---|---|---|
| `GET /healthz` | – | liveness: process up, no DB | 200 `{"status":"ok"}` |
| `GET /readyz` | – | readiness: `SELECT 1` with a 2 s timeout, and not shutting down | 200 / 503 |
| `POST /v1/auth/register` | – | `{username, password, client_label?}` → creates the user, session and recovery code | 201 `{user, session_token, recovery_code}` |
| `POST /v1/auth/login` | – | `{username, password, client_label?}` | 200 `{user, session_token}` |
| `POST /v1/auth/logout` | user | revoke the current session | 204 |
| `POST /v1/auth/recover` | – | `{username, recovery_code, new_password}` → resets the password, revokes all sessions, issues a new recovery code | 200 `{user, session_token, recovery_code}` |
| `GET /v1/me` | user | own profile | 200 `User` |
| `POST /v1/me/password` | user | `{current_password, new_password}` → revokes the other sessions | 204 |
| `POST /v1/me/recovery-code` | user | `{current_password}` → rotates the recovery code | 200 `{recovery_code}` |
| `GET /v1/me/sessions` | user | list active sessions | 200 `{items:[{id, client_label, created_at, last_used_at, current}]}` |
| `DELETE /v1/me/sessions/{id}` | user | revoke one of your own sessions (someone else's → 404) | 204 |
| `DELETE /v1/me` | user | `{password}` → delete the account (§H.5) (Phase 5) | 202 |
| `GET /v1/me/export` | user | JSON export of your profile + messages (Phase 5) | 200 |
| `GET/PUT/DELETE /v1/me/blocks[/{username}]` | user | blocks (Phase 5) | 200/204 |
| `POST /v1/discussions/resolve` | optional | `{url, canonical_url?}` → lookup only, **never creates** | 200 `{identity, discussion \| null}` |
| `POST /v1/discussions` | user | `{url, canonical_url?, title?}` → create or return existing | 201 / 200 `Discussion` |
| `GET /v1/discussions/{id}` | optional | metadata (follows a merge → returns the target with `merged_from`) | 200 `Discussion` |
| `GET /v1/discussions/{id}/messages` | optional | paginated history | 200 page |
| `POST /v1/discussions/{id}/messages` | user | `{body, parent_id?, client_msg_id}` | 201 / 200 `Message` |
| `GET /v1/messages/{id}` | optional | one message | 200 `Message` |
| `DELETE /v1/messages/{id}` | author | soft-delete your own message | 204 |
| `POST /v1/reports` | user | `{target_type, target_id, reason, details?}` (Phase 5) | 201 / 200 |
| `GET /v1/users/{username}` | optional | public profile `{id, username, created_at}` | 200 |
| `GET /v1/mod/reports?status=open&cursor=` | mod | report queue (Phase 5) | 200 page |
| `POST /v1/mod/reports/{id}/resolve` | mod | `{outcome: actioned\|dismissed, note?}` | 200 |
| `POST /v1/mod/messages/{id}/remove` · `/restore` | mod | `{reason, report_id?}` | 200 |
| `POST /v1/mod/users/{id}/suspend` · `/unsuspend` | mod | `{reason, until?, report_id?}` (moderators cannot suspend admins or other moderators) | 200 |
| `POST /v1/mod/discussions/{id}/lock` · `/unlock` · `/hide` · `/unhide` | mod | `{reason}` | 200 |
| `GET /v1/realtime` | optional | WebSocket upgrade (§G) | 101 |
| `GET /docs`, `/openapi.json` | – | Scalar UI + spec (**only when `YAPSPACE_ENV=dev`**) | 200 |

### F.3 DTOs
```jsonc
// User (own, from /me)          // PublicUser (embedded)
{"id": "…", "username": "alice", "role": "user", "created_at": "…"}      {"id": "…", "username": "alice"}
// Discussion
{"id": "…", "display_url": "https://example.com/a", "site": "example.com", "title": "Article title",
 "status": "open", "message_count": 12, "last_seq": 12, "created_at": "…", "last_message_at": "…", "merged_from": null}
// Identity (from resolve)
{"kind": "canonical", "display_url": "https://example.com/a", "site": "example.com", "ruleset": 1}
// Message. body is null when status != visible; author is null when the author is deleted
{"id": "…", "discussion_id": "…", "seq": 42, "author": {"id": "…", "username": "bob"},
 "parent": {"id": "…", "seq": 40, "author_username": "alice", "excerpt": "first 140 chars…", "status": "visible"} ,
 "body": "hello", "status": "visible", "created_at": "…", "edited_at": null}
```
Validation: `username` is lowercased first, then must match `^[a-z0-9_]{3,24}$`, must not be in `RESERVED_USERNAMES` (`admin administrator mod moderator
yapspace support help system root api staff official null undefined anonymous`), and must not start with `deleted_`. `password` is 10–128 chars and
must not equal the username. `body`: NFC normalize, trim, reject C0/C1 control chars except `\n`/`\t`, collapse 3+ consecutive newlines to 2, then
1–2000 chars. `title`: strip control chars, collapse whitespace, truncate to 300. `client_label`: truncate to 100.

### F.4 Schema generation
`utoipa` derives (`ToSchema` on DTOs, `#[utoipa::path]` on handlers, collected with `utoipa_axum::router::OpenApiRouter` + `routes!`).
`yapspace openapi` prints the JSON. `task api:openapi` writes `docs/api/openapi.json`, which is **committed**, and CI fails if regenerating it changes the file.
The extension generates `extension/src/lib/api/schema.d.ts` with `openapi-typescript` and calls the API with `openapi-fetch`, so TS types
are never hand-written.
