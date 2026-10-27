# 0006 Event distribution

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: event distribution.

## Decision
In-process broadcast per discussion now. An EventBus trait later, with Postgres LISTEN/NOTIFY, then NATS if NOTIFY saturates.

## Alternatives considered
- Redis Pub/Sub: another stateful service for one job.
- Kafka: far more than we need.

## Reasons
- Lost events are recovered from the database through after_seq, so the transient layer can be simple.

## Consequences
- Multi-instance deployments need the NOTIFY bus before they are correct (P6-02).

## Revisit when
- The triggers in capacity.md are reached.
