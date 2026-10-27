# 0003 Modular monolith

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: modular monolith.

## Decision
One binary with internal modules (api, domain, db, realtime). One deployable.

## Alternatives considered
- Separate services for realtime or auth: more deployment and failure modes for a small team.

## Reasons
- A single process is simple to run, back up and debug.
- Module boundaries keep the option to split later.

## Consequences
- Any module can slow the others down, so limits are per module.

## Revisit when
- Realtime or search needs to scale on its own.
