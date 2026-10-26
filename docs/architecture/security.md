# Security, privacy & abuse

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## H. Security, privacy & abuse

### H.1 Authentication
- **Passwords:** `argon2` crate, Argon2id, `m=19456 KiB, t=2, p=1` (OWASP minimum), PHC string. Hashing runs in `spawn_blocking`,
  gated by a `Semaphore(4)` (so it can't exhaust the CPU, which would be a DoS). Rehash on login if the parameters changed.
- **Login timing/enumeration:** an unknown username still runs `verify` against a fixed dummy hash, and the response is always `invalid_credentials`.
  Registration necessarily reveals "username taken". That is acceptable for public usernames and is mitigated by the registration rate limit.
- **Session token:** 32 random bytes from `rand::rngs::OsRng` → base64url (no padding) → prefixed `ys_`. Only `sha256(token)` is stored.
  TTL: 30 days sliding, 180 days absolute. Logout revokes the session. A password change or recovery revokes all *other* (or all) sessions.
  Suspension doesn't revoke sessions but blocks writes. Deletion revokes everything.
- **Recovery code:** 20 chars of Crockford base32 (100 bits), shown once, formatted `XXXXX-XXXXX-XXXXX-XXXXX`. Normalize the input by uppercasing and removing
  dashes and spaces. Store `sha256`. It's single-use: using it rotates it.
- **Authorization:** the extractors `AuthUser` (401 if missing or invalid), `MaybeAuthUser`, and `ModUser`/`AdminUser` (403). Ownership checks are
  in the domain layer (`message.author_id == user.id`). Anything not owned → **404, not 403** (no IDOR existence oracle). Every
  protected endpoint has a test for no auth, wrong user, and a suspended user.
- External IdPs were rejected for the MVP (vendor coupling, redirect flows that differ per browser, privacy). OIDC can be added later as a new
  `auth_identities` table, without touching `users`.

### H.2 Application security checklist (each item has a test or a CI check)
| Threat | Mitigation |
|---|---|
| SQL injection | sqlx bound parameters only. sqlx 0.9 `SqlSafeStr` makes dynamic SQL explicit (`AssertSqlSafe`). Grep in CI: no `AssertSqlSafe` without a `// SAFETY:` comment |
| XSS (extension) | Svelte text interpolation only. **`{@html}` is banned** (an eslint rule plus a grep in CI). Links come from a tokenizer, http(s) only, `rel="noopener noreferrer nofollow ugc"` |
| CSRF | Bearer tokens, no cookies |
| SSRF | The server never fetches user URLs (no previews in the MVP). Previews later need an allowlisted fetcher with private-IP blocking |
| Auth bypass / IDOR | Extractors on every route, plus the authz matrix test (P2-22) |
| Proxy trust / spoofed IP | `X-Forwarded-For` is honored only when the peer IP is in `YAPSPACE_TRUSTED_PROXIES`. Take the **right-most** entry |
| Request smuggling | Caddy is the only ingress, with HTTP/1.1 + h2 to clients and HTTP/1.1 to upstream. Body limits at Caddy (64 KiB) and the app (16 KiB) |
| WS abuse | Origin allowlist, per-IP/global caps, frame size/rate limits, slow-consumer eviction |
| Resource exhaustion | Timeouts (HTTP 10 s, DB statement_timeout 5 s via the connection `options`), pool acquire timeout 3 s, argon2 semaphore, `limit` caps |
| Malformed input | `deny_unknown_fields`, typed extractors, a custom JSON rejection → 400 problem+json |
| Dependency vulns | `cargo deny check` (advisories, licenses, bans), `npm audit --omit=dev`, Trivy image scan, Dependabot |
| Secrets exposure | Secrets come from files (`*_FILE` env vars), `.env*` is gitignored except the examples, and tokens are never logged (a `Debug` impl redacts them) |
| Privilege escalation | Roles change only through the CLI (`yapspace admin set-role`) → audit event. Moderators can't act on moderators or admins |

### H.3 Extension security
- **Permissions:** `activeTab`, `scripting`, `storage`, and `host_permissions` for the API origin only. **No `<all_urls>`, no `tabs`.** The panel is injected
  only after a user gesture (toolbar click or keyboard command).
- **Isolation:** the page can't read the extension iframe's DOM (cross-origin). The content script only *reads* `location`, `<link rel=canonical>`,
  `og:url` and `document.title`, and creates a host element with a **closed** shadow root containing the iframe. Nothing from the page is
  rendered as HTML.
- **Messaging:** content script ↔ background via `runtime.sendMessage` (the page can't reach it). Panel ↔ background via `runtime.connect`
  port. The background validates `sender.id === browser.runtime.id` and the message shape (a TS type guard). **`window.postMessage` is never used**
  (pages can spoof it).
- **Token storage:** `storage.local` key `ys.session`. Only extension contexts read it, and the content script never touches it. The token is sent only to the
  configured API origin. Logout clears it.
- **Fingerprinting:** `panel.html` has to be web-accessible. On Chromium use `use_dynamic_url: true`. On Firefox, the moz-extension UUID is random per install.
- **CSP:** the extension pages use the MV3 default CSP (no remote code, no eval).

### H.4 Spam & abuse controls (MVP defaults, configurable, all in-process via `governor`)
| Bucket | Key | Limit |
|---|---|---|
| register | IP | 5 / hour |
| login | IP | 30 / 15 min; and IP+username 10 / 15 min |
| recover | IP | 5 / hour |
| resolve | IP (or user) | 120 / min |
| create discussion | user | 30 / hour |
| post message | user | burst 5, refill 1 per 3 s (≈20/min); **accounts < 24 h old: burst 3, refill 1 per 30 s** |
| post message | IP | 60 / min (backstop for multiple accounts) |
| report | user | 30 / day |
| ws connect | IP | 20 concurrent, 30 new / min |
| global per-IP | IP | 600 req / min (backstop) |
- **Duplicate spam:** a `moka` cache `(user_id, sha256(lowercased, whitespace-collapsed body))`, TTL 10 min. A second identical body anywhere →
  `422 validation_failed` with code `duplicate_message`. Ten distinct discussions in 5 minutes for an account < 24 h old → 429.
- **Shared networks and VPNs:** IP limits are deliberately loose backstops. Real limits are per account. IPs are kept in memory only (never written
  to the DB or logs).
- **Moderation:** reports → queue → mod actions (remove/restore, suspend with an optional `until`, lock/hide discussion), all recorded in `moderation_actions`.
  Removed messages show as "removed by moderators". Blocks (Phase 5) hide the blocked user's messages on the client side.
- **Later:** content heuristics (link density, new-account links), hashed-IP reputation, a captcha on registration at an anomaly threshold, auto-hide at N
  reports from trusted users.

### H.5 GDPR & privacy (architecture recommendations, **not legal advice**; D-05/D-06 need a specialist)
- **Data minimization:** no email, no IP in the DB or logs, no raw URLs stored, no analytics/telemetry from the extension, and no cookies (so no
  cookie banner is needed for the API. Verify this with counsel). `client_label` is free text set by the client (e.g. "Chrome extension").
- **Personal data held:** username, password hash, recovery hash, sessions (label and timestamps), messages, reports, mod/audit records.
- **Account deletion** (`DELETE /v1/me`, D-04 recommended): in one transaction set `status='deleted'`, `username='deleted_'+first 12 hex chars of the id`,
  `deleted_at=now()`, delete the credentials and sessions, and write an audit event. Then the **purge job** erases bodies in batches of 1000:
  `UPDATE messages SET body='', status='deleted', status_changed_at=now() WHERE author_id=$1 AND status<>'deleted'`. Tombstones keep
  replies coherent. Reports the user *filed* are kept, linked to the anonymized user.
- **Export** (`GET /v1/me/export`): JSON with the profile, sessions, and all messages (id, discussion display_url, seq, body, status, timestamps), streamed.
- **Retention:** §D.4. IPs live only in memory, in the rate limiters (cleared by `retain_recent` every minute).
- **Sensitive URLs:** §E.6–E.7 (refusal rules, blocked capability hosts, POST resolve, nothing raw is stored).
- **Logs:** request method, the *route template* (not the raw path), status, latency and request_id. No query strings, bodies, tokens or IPs.
- **Processors:** the VPS host and the backup storage provider (DPAs needed). Backups are encrypted client-side (restic) before they leave the server.
- **Public disclosures:** privacy notice + terms (P5-14), and the extension store privacy disclosures ("website content: URL of the active tab,
  sent only when you open YapSpace").

### H.6 Threat model (summary)
- **Assets:** account credentials and sessions, message integrity, user privacy (browsing URLs), service availability, admin powers, backups.
- **Trust boundaries:** (1) web page ↔ content script, (2) content script ↔ extension pages/background, (3) extension ↔ API over
  the internet, (4) Caddy ↔ API (internal network), (5) API ↔ Postgres, (6) server ↔ off-site backup storage, (7) admin shell.
- **Top threats → mitigations → residual risk:**
  1. A malicious page manipulates the context (fake canonical or title) → same-site canonical rule, the title is text only, the first poster sets the title → *Residual:* misleading titles (reportable).
  2. Spam floods → per-account limits, new-account limits, dup detection, reports → *Residual:* determined multi-account spam (add captcha/reputation later).
  3. Credential stuffing → per-IP+username limits, argon2id → *Residual:* reused passwords (no MFA in the MVP; add TOTP later).
  4. Session theft from a compromised device → revocable sessions, session list UI → *Residual:* a token stays valid until revoked.
  5. Privacy leak through URLs → refusal rules, no raw URL storage → *Residual:* new capability-URL patterns (blocked-host list is updatable).
  6. DoS → Caddy + app limits, WS caps → *Residual:* volumetric attacks need upstream DDoS protection (Cloudflare in front of the API is an option later. WS works through it).
  7. Server compromise → non-root distroless container, internal network, secrets in files, encrypted off-site backups → *Residual:* single-host blast radius.
  8. Data loss → daily encrypted off-site backups + weekly restore test → *Residual:* RPO ≤ 24 h (WAL archiving later for a smaller RPO).
