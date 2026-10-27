# 0014 Search

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: search.

## Decision
No search in the MVP. Postgres full-text search first. A dedicated engine only after measured need.

## Alternatives considered
- OpenSearch or Meilisearch now: another stateful service with no measured demand.

## Reasons
- Search can be added without changing the data model (a tsvector column and a GIN index).

## Consequences
- No search until P6-08.

## Revisit when
- Product demand and the limits of Postgres full-text search are both shown.
