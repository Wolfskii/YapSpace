# 0001 Backend framework

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: backend framework.

## Decision
axum 0.8 on tokio, with tower-http middleware.

## Alternatives considered
- actix-web: as fast, but it brings its own actor model and a smaller tower middleware ecosystem.
- poem, salvo, rocket: smaller communities and fewer middleware examples for our needs.

## Reasons
- axum is built on tower, so middleware, timeouts and body limits are shared with the ecosystem.
- Typed extractors make auth and validation explicit per handler.

## Consequences
- We are tied to the tower model.
- Breaking changes in axum need a migration pass (0.7 to 0.8 needed one).

## Revisit when
- axum is abandoned, or we need something it cannot do.
