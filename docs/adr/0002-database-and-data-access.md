# 0002 Database and data access

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: database and data access.

## Decision
PostgreSQL 18 with sqlx 0.9. Compile-time checked query macros, an offline query cache in .sqlx/, and embedded migrations.

## Alternatives considered
- Diesel: a sync core (diesel-async is separate) and a heavier DSL.
- SeaORM: an ORM layer we do not need, and weaker compile-time SQL checking.
- CockroachDB, Yugabyte, MongoDB, Cassandra: distributed or document stores. Relational integrity and SQL tooling matter more here (see data-model.md section 5).

## Reasons
- Hand-written SQL with checked types keeps queries visible and reviewable.
- The offline cache lets the Docker build run without a database.

## Consequences
- Every query change needs `cargo sqlx prepare` (documented in AGENTS.md).
- Migrations are immutable once committed.

## Revisit when
- The write volume breaks the triggers in capacity.md, or sqlx maintenance stalls.
