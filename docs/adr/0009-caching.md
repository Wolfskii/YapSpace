# 0009 Caching

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: caching.

## Decision
No cache in the MVP. An in-process moka cache later for hot reads. Redis only when several instances must share state.

## Alternatives considered
- Redis now: no measured need, and one more thing to run.

## Reasons
- The database with indexes is enough at the planned scale (see capacity.md).

## Consequences
- Hot pages hit the database harder until P6-04.

## Revisit when
- Measured read load justifies it (capacity.md section L.4).
