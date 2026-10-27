# 0005 Realtime WebSocket

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: realtime websocket.

## Decision
A WebSocket for subscriptions only. Writes go over REST. Resume uses seq cursors (after_seq).

## Alternatives considered
- SSE: one-way, and subscribe would need extra HTTP calls. HTTP/1.1 connection limits.
- Polling: more server work and worse latency.

## Reasons
- Writes stay on one validated, idempotent path.
- Resume is simple: the client sends the last seq it has.

## Consequences
- Each socket costs memory (see capacity.md).
- Frames need limits (size and rate).

## Revisit when
- A client platform cannot use WebSockets.
