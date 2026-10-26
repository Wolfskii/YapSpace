# Architecture Decision Records

Index and summaries. Task P0-03 expands each row into `NNNN-slug.md` using the template below.

## O. Architecture Decision Records (summaries; P0-03 expands each one into `docs/adr/NNNN-slug.md`)

ADR file template: `# NNNN Title` · `Status: Accepted | Provisional (validate in <task>)` · `Date` · `## Context` · `## Decision` ·
`## Alternatives considered` · `## Reasons` · `## Consequences` · `## Revisit when`. Each one is 40–90 lines and uses the content below plus the referenced section.

| # | Title / slug | Decision | Status | Revisit when |
|---|---|---|---|---|
| 0001 | backend-framework | axum 0.8 on tokio, tower-http middleware (§A) | Accepted | axum gets abandoned, or we need something axum can't do (unlikely) |
| 0002 | database-and-data-access | PostgreSQL 18 + sqlx 0.9 macros, embedded migrations, offline cache (§A, §D) | Accepted | the write volume breaks the scaling triggers in §L.4, or sqlx maintenance stalls |
| 0003 | modular-monolith | one binary with internal modules, one deployable (§C.1) | Accepted | independent scaling or release cadence is needed for some part (e.g. realtime) |
| 0004 | rest-api-with-openapi | REST /v1 + problem+json + keyset pagination + utoipa OpenAPI → TS types (§F) | Accepted | clients need flexible aggregation that REST can't serve efficiently |
| 0005 | realtime-websocket | WS subscribe channel, writes over REST, seq-based resume (§G) | Accepted | platform constraints (e.g. a client that can't use WS) |
| 0006 | event-distribution | in-process broadcast now. `EventBus` trait → PG NOTIFY → NATS (§G.3, §L.4) | Accepted | the triggers in §L.4 |
| 0007 | url-identity | versioned deterministic keys, server-authoritative, adapters, no implicit aliasing, admin-only merges (§E) | **Provisional** (validate with real browsing data in P6-01) | the false-split/false-merge rate from `wrong_discussion` reports is > 1% of discussions |
| 0008 | authentication | username+password (argon2id), opaque revocable bearer sessions, recovery code, no email (§H.1) | Accepted | an email requirement appears (recovery/notifications) or OIDC demand |
| 0009 | caching | no cache in the MVP. moka in-process later. Redis only for shared state across instances (§L.4) | Accepted | the triggers |
| 0010 | docker-and-reverse-proxy | Compose on one VPS, Caddy, distroless image, one-shot migrate service (§J) | Accepted | HA requirements, or more than one host |
| 0011 | extension-architecture | WXT + Svelte 5, activeTab injection, panel in an extension iframe, background routes context (§I) | **Provisional** (validate in the P4-02 spike) | the iframe is blocked on important sites, or the side-panel UX tests better |
| 0012 | repository-structure | monorepo: crates/, extension/, spec/, deploy/, docs/ (§N) | Accepted | the team grows and needs separate release trains |
| 0013 | observability | tracing JSON logs + Prometheus metrics + optional Grafana profile, OTel deferred (§K) | Accepted | multiple services or hosts → add OTLP traces |
| 0014 | search | no search in the MVP. Postgres FTS first, a dedicated engine only on measured need (§L.4) | Accepted | product demand plus FTS limits |
| 0015 | desktop-framework | Tauri 2 for the desktop app (Phase 7). Reuses `yapspace-identity` and the REST/WS API | **Provisional** | Tauri webview limitations on Linux, or a product pivot |
