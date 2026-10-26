# Phase 5 · Security & production readiness

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** Phase 4 PASSED. D-01/D-02 decided before P5-09/P5-10 (otherwise those tasks are `BLOCKED`, while the others continue).
**Parallelizable:** the backend (P5-02, P5-03, P5-05..P5-07) and ops (P5-09..P5-11) tracks.
**Exit criteria:** the §S Definition of Done holds.

### P5-01 · Threat model review
**Priority:** Must · **Deps:** — (start of phase) · **Spec:** §H.6
**Deliverables:** a pass over `docs/architecture/security.md`: for each threat, link the test or config that implements its mitigation (the file path). List any gaps as
new board tasks `P5-xx` (append them, don't renumber).

### P5-02 · Reports API
**Priority:** Must · **Deps:** P2-22 · **Spec:** §D.2 reports, §F.2
**Deliverables:** `POST /v1/reports` (validate that the target exists in the same tx: message → messages, user → users not deleted, discussion → discussions;
duplicate → 200 existing; rate limit `report`). Flip `FEATURES.reports = true` in the extension.
**Tests:** create, duplicate → 200, a nonexistent target → 404, rate limit.

### P5-03 · Moderation API + enforcement
**Priority:** Must · **Deps:** P5-02 · **Spec:** §F.2 mod routes, §D.2 moderation_actions
**Deliverables:** every `/v1/mod/*` route. Each action runs in one tx: change the target + insert `moderation_actions` + (if `report_id` is given) resolve the report.
Then publish `message.updated` / `discussion.updated` after commit. Suspension sets `status='suspended', suspended_until` (null = indefinite). Moderators
can't target moderators/admins (403). Unsuspend clears it. The report queue uses a keyset cursor `(created_at, id)`, base64 encoded.
Removed bodies are kept for 90 days (the purge is in P5-07).
**Tests:** a user calling a mod route → 403. A mod removes a message → a WS subscriber gets an update with status removed and body null. A suspended user posting → 403
`account_suspended`. A moderator suspending an admin → 403. A moderation_actions row exists for every action.

### P5-04 · Moderation UI (extension)
**Priority:** Should · **Deps:** P5-03
**Deliverables:** for moderators (role from `/v1/me`): inline "Remove"/"Restore" in the message menu, "Lock discussion" in the header menu, and an options page tab
"Reports" (the queue with the target preview, actions, and a resolve/dismiss button).
**Tests:** component tests showing the mod actions only appear for role moderator/admin.

### P5-05 · Blocks
**Priority:** Should · **Deps:** P5-02
**Deliverables:** migration `0002_user_blocks.sql` (§D.2), `GET /v1/me/blocks`, `PUT /v1/me/blocks/{username}`, `DELETE /v1/me/blocks/{username}`
(max 1000 blocks per user). Extension: a "Block user" action → hides their messages behind "Message from a blocked user (show)". The block list
is cached in the session store.
**Tests:** API CRUD, a self-block → 422, the client hiding logic.

### P5-06 · Account deletion + export
**Priority:** Must · **Deps:** P5-03 · **Spec:** §H.5
**Deliverables:** `DELETE /v1/me` `{password}` → 202 (the tx from §H.5 + audit `user.deleted`), `GET /v1/me/export` (streamed JSON: write the opening part, then
stream the messages in batches of 500 using keyset on `(author_id, created_at, id)`). Extension Settings: change password, rotate the recovery code, sessions
list with revoke, export download, delete account (type your username + password to confirm).
**Tests:** after deletion, login fails, the username shows as `deleted_…` and the author is null in the API, the purge job erases the bodies (run the job function directly),
and the export contains every message of the user.

### P5-07 · Background jobs
**Priority:** Must · **Deps:** P5-06
**Deliverables:** `jobs.rs`: a `spawn_jobs(state)` loop registered in the TaskTracker with cancellation. Jobs: `purge_sessions` (hourly: delete where `expires_at
< now() - 30d OR revoked_at < now() - 30d`), `purge_deleted_users_content` (every minute, batch 1000), `purge_removed_bodies` (daily: `body=''` where
`status='removed' AND status_changed_at < now() - 90d AND body <> ''`), `purge_reports` (daily: resolved > 1 y), `purge_audit` (daily: > 2 y),
`ratelimit_retain` (every minute). Each logs and records `jobs_runs_total{job,result}`. An advisory lock (`pg_try_advisory_lock(hashtext(job))`) makes them safe
with several instances later.
**Tests:** each job function is called directly against seeded data.

### P5-08 · Hardening pass
**Priority:** Must · **Deps:** P5-03
**Deliverables:** confirm every §H.2 row has a test or CI guard (add the missing ones). `cargo deny check` clean. `npm audit --omit=dev` has no high-severity findings. Trivy clean.
Add a `SECURITY.md` (how to report a vulnerability). Check that tokens are redacted from logs (a test capturing the logs during login).

### P5-09 · Production compose + Caddy
**Priority:** Must · **Deps:** P1-09, D-01, D-02 · **Spec:** §J.1–J.5
**Deliverables:** `deploy/compose.yaml`, `deploy/Caddyfile`, `deploy/.env.example`, `deploy/postgres/init/01-extensions.sql`,
`deploy/scripts/{gen-secrets.sh, smoke.sh, backup-now.sh}`, `deploy/README.md` (server bootstrap: an Ubuntu 24.04 LTS VPS, a non-root deploy user, SSH keys only,
ufw 22/80/443, unattended-upgrades, Docker Engine + compose plugin, `/opt/yapspace` layout, DNS A/AAAA records, first start).
`smoke.sh`: curl `/readyz`, register a throwaway `smoke_<random>` user → post on a smoke discussion → delete the account.
**Acceptance:** `docker compose -f deploy/compose.yaml config` validates. A dry run locally with `YAPSPACE_DOMAIN=localhost` (Caddy's internal CA) works end to end, including WS.
Then on the real VPS: set `IN_REVIEW` until the user confirms that the TLS cert was issued.

### P5-10 · Backups + restore test
**Priority:** Must · **Deps:** P5-09 · **Spec:** §J.6
**Deliverables:** `deploy/backup/{Dockerfile, backup.sh, restore-test.sh, crontab}` (supercronic: `15 3 * * *` backup, `45 4 * * 0` check + restore
test), `deploy/backup.env.example`, `task ops:restore-test`, `docs/runbooks/backup-restore.md` (a full restore onto a new host, step by step).
**Acceptance:** a backup appears in the restic repo. `restore-test.sh` reports OK. The heartbeat URL is pinged. A deliberately broken repo password → the job fails and
**no** heartbeat is sent.

### P5-11 · Monitoring profile + alerts
**Priority:** Should · **Deps:** P5-09 · **Spec:** §K
**Deliverables:** in `deploy/compose.yaml`, under `profiles: [monitoring]`: prometheus (15 d retention, scraping api:9090, node-exporter, postgres-exporter),
grafana (provisioned datasource + dashboard JSON `deploy/grafana/yapspace.json` with panels for every §K metric + alert rules from §K),
bound to `127.0.0.1:3000` only (reach it through an SSH tunnel). An external uptime check is documented.
**Acceptance:** the dashboard shows live data after `docker compose --profile monitoring up -d`. A test alert fires to the configured contact point.

### P5-12 · Load & soak validation
**Priority:** Must · **Deps:** P5-09, P3-09 · **Spec:** §L.5 pass criteria
**Deliverables:** run browse, viral, reconnect-storm and a 2 h soak against the production-like compose (staging on the VPS before launch, or a same-size
VPS). Record the results in `capacity.md` "Measured (staging)". Tune the limits/pool if needed (document the changes). File issues for any regressions.
**Acceptance:** the §L.5 pass criteria are met, or deviations are documented with an accepted mitigation (`NEEDS_DECISION` if not acceptable).

### P5-13 · Runbooks + DR drill
**Priority:** Must · **Deps:** P5-10
**Deliverables:** `docs/runbooks/{deploy.md, rollback.md, backup-restore.md, incident.md, secrets-rotation.md, disk-full.md, ban-wave.md}`. Do the DR
drill once: rebuild a fresh VPS from the backup using only the runbooks. Record the time taken (target < 2 h).

### P5-14 · Privacy notice, terms, store disclosures
**Priority:** Must · **Deps:** P5-06 · **Spec:** §H.5
**Deliverables:** `docs/legal/privacy-notice.md` and `docs/legal/terms.md` as **drafts marked "requires legal review"** (D-05/D-06), the in-extension links
(Settings → Privacy / Terms → hosted URLs once D-01 is decided), and the store privacy answers in `store-submission.md`.
**Acceptance:** the drafts exist. The board shows D-05/D-06 status; the launch is blocked until they're `DECIDED`.

### P5-15 · Release workflow + deploy task
**Priority:** Must · **Deps:** P5-09, P1-10
**Deliverables:** `.github/workflows/release.yml` (§M.3), `CHANGELOG.md` (Keep a Changelog), `task release VERSION=0.1.0` (bump the extension
`package.json` version + the workspace version → commit `chore: released v0.1.0` → tag `v0.1.0`), `task deploy TAG=v0.1.0` (SSH host from the `.env` var
`DEPLOY_HOST`; runs the §J.6 sequence), `deploy/scripts/upgrade-test.sh`.
**Acceptance:** a tag push builds and pushes the image + the release zips (needs D-03). A deploy to staging works. A rollback to the previous tag works.

### P5-16 · Final security review + MVP sign-off
**Priority:** Must · **Deps:** every other P5 task
**Deliverables:** `docs/reviews/mvp-security-review.md`: go through §H.2–H.6 and the OWASP ASVS L1-relevant items, the extension permission audit, a dependency audit, and a confirmation of every
item in §S with evidence (test names, screenshots, metrics). Any open risk is accepted explicitly by the user.
**Acceptance:** every item in §S is checked with evidence. Phase 5 gate PASSED → **MVP launch-ready**.
