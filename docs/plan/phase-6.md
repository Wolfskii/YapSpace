# Phase 6 · Early growth (each task starts with a measurement; it's skipped if the trigger isn't met)

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** the MVP is live with ≥ 2 weeks of production metrics. Every task first checks its trigger in §L.4. If the trigger isn't met → `DEFERRED` with the
measured numbers in Notes.
- **P6-01 · Usage & identity review** (Must): a weekly report from `pg_stat_statements`, the endpoint latency, the `wrong_discussion` report rate, and the top sites
  without adapters. Output: `docs/reports/YYYY-MM-DD-usage.md` + new adapter/rule proposals (as NEEDS_DECISION items for a ruleset bump).
- **P6-02 · `PgNotifyBus` + multi-instance** (Could → Must when triggered): `bus.rs` `PgNotifyBus` (a dedicated `PgListener` on `yapspace_events`, reconnect with
  backoff; on reconnect, the local hubs send `resync_required{lagged}` to every subscription so clients catch up via after_seq). Config `YAPSPACE_EVENT_BUS=local|pg`.
  Compose `api` gets `deploy.replicas: 2`, and Caddy `reverse_proxy api:8080` load-balances across the DNS round-robin. Rate limits become approximate per instance
  (documented). Tests: two app instances in one test process sharing the DB; a post on A is delivered to a subscriber on B.
- **P6-03 · Zero-downtime deploys** (Could): rolling restart with 2 replicas + Caddy health checks (`health_uri /readyz`).
- **P6-04 · Read cache** (Could): moka caches for `resolve` (key_hash → discussion id, TTL 60 s, negative entries TTL 10 s) and for the newest page per discussion
  (invalidated on publish/update). Measure the hit rate before and after.
- **P6-05 · Slow mode for hot discussions** (Should): when the per-discussion write rate is > N/min (config), apply a per-user per-discussion cooldown
  (e.g. 1 post / 10 s) + a `discussion.updated{slow_mode:true}` event. The UI shows "Slow mode".
- **P6-06 · DB scaling analysis** (Could): a document evaluating a read replica (for history reads) and partitioning `messages` (hash by discussion_id) against the
  measured numbers. Implement it only if triggered.
- **P6-07 · Admin discussion merge** (Should): `yapspace admin merge-discussion <from> <into>` per §E.8 + a "merged" banner in the extension.
- **P6-08 · Search** (Could): Postgres FTS on messages (`tsvector` generated column `simple` config + a GIN index), `GET /v1/discussions/{id}/search?q=` within one
  discussion first. Respects tombstones (filter on status). Separate engine only per §L.4.
- **P6-09 · Reactions** (Could): `message_reactions(message_id, user_id, emoji text CHECK in an allowlist of 8, created_at, PK(message_id,user_id,emoji))`,
  aggregated counts in the message DTO via a lateral subquery limited to the page, plus a `reaction.updated` WS event.
- **P6-10 · Page badges (optional host permission)** (Could): `optional_host_permissions: ['<all_urls>']`, an opt-in setting → the background resolves
  the current tab (debounced) via `POST /v1/discussions/resolve-batch` (≤ 20 URLs, returning only counts) and shows the message count on the action badge.
  Privacy: off by default, clearly explained.
- **P6-11 · Content adapters for feeds** (Could): `extension/src/adapters/x.ts`, `reddit.ts`: a "Discuss" button on posts in feeds opens the panel with the post
  permalink as the context URL. They fail closed. Each adapter has fixture-based tests.
- **P6-12 · Side panel surface** (Could): Chrome `sidePanel` / Firefox `sidebar_action` reusing `panel.html`, chosen in settings.
- **P6-13 · Abuse automation** (Should): a captcha on registration (self-hosted, e.g. ALTCHA proof-of-work, which needs no third party) when the registration rate is
  anomalous; link-spam heuristics for new accounts; auto-hide after 3 reports from accounts > 30 days old, pending review.
