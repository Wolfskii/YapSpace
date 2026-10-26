# Real-time protocol

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## G. Real-time protocol

### G.1 Transport decision
**WebSocket** at `wss://<api>/v1/realtime`, JSON text frames, protocol version 1. Clients **post through REST**. The socket is a
subscription channel only (plus control frames). That keeps writes on one validated path, makes idempotency and rate limits work the same way everywhere,
and keeps the socket logic small. SSE was rejected because subscribe/unsubscribe would need separate HTTP calls, there are per-origin connection limits on HTTP/1.1,
and WS proxies through Caddy with no extra config. Polling was rejected because it costs more server work per user at worse latency.

### G.2 Frames
Client → server (≤ 4 KiB per frame, ≤ 20 frames per 10 s, otherwise close 4000):
```jsonc
{"type": "hello", "protocol": 1, "client": "yapspace-ext/0.1.0", "token": "ys_…"}   // optional; token optional
{"type": "subscribe", "discussion_id": "…", "after_seq": 120}                        // after_seq = last seq the client has
{"type": "unsubscribe", "discussion_id": "…"}
{"type": "ping", "nonce": 7}
```
Server → client:
```jsonc
{"type": "welcome", "protocol": 1, "conn_id": "…", "heartbeat_s": 25, "max_subscriptions": 5}
{"type": "hello_ok", "user_id": "…" | null}                     // after hello; invalid token → error code unauthorized, connection stays anonymous
{"type": "subscribed", "discussion_id": "…", "last_seq": 130}  // sent after the backlog has been replayed
{"type": "message.created", "discussion_id": "…", "seq": 131, "message": { /* Message DTO */ }}
{"type": "message.updated", "discussion_id": "…", "seq": 99,  "message": { /* Message DTO, e.g. status=removed */ }}
{"type": "discussion.updated", "discussion_id": "…", "discussion": { /* Discussion DTO (lock/hide) */ }}
{"type": "resync_required", "discussion_id": "…", "reason": "gap_too_large" | "lagged"}
{"type": "pong", "nonce": 7}
{"type": "error", "code": "bad_frame" | "unknown_discussion" | "too_many_subscriptions" | "unauthorized" | "rate_limited", "detail": "…"}
```
Close codes: 1000 normal · 1001 going away · 1008 origin rejected · 1009 frame too big · 1011 internal · **1012 server restart** (the client
reconnects with jitter) · 4000 protocol error · 4004 idle timeout · 4008 slow consumer · 4029 too many connections.

### G.3 Server design
- **Hub** (`realtime/hub.rs`): `DashMap<Uuid, broadcast::Sender<Arc<Event>>>` where
  `struct Event { discussion_id: Uuid, kind: EventKind /* Created | Updated | DiscussionUpdated */, seq: i64, payload: Utf8Bytes }`.
  The payload is serialized **once** per event, and every socket gets a cheap clone of `Utf8Bytes`. Channel capacity: 1024.
  `publish()` on a discussion nobody watches is a no-op (no sender → nothing to do). `subscribe()` creates the sender lazily.
  When a subscription ends, `if sender.receiver_count() == 0 { map.remove_if(...) }`.
- **`EventBus` trait** (`realtime/bus.rs`): `async fn publish(&self, ev: Event)`. MVP impl `LocalBus` calls `hub.publish` directly.
  Phase 6 adds `PgNotifyBus`: `NOTIFY yapspace_events, '{"d":"<uuid>","s":131,"k":"c"}'` (payload ≪ the 8000-byte limit). Each instance
  `LISTEN`s on a dedicated connection, loads the message by `(discussion_id, seq)` from the DB, and publishes it to its local hub.
  **The DB is the source of truth. A lost notification is recovered by the client's `after_seq` resume.**
- **Connection task** (`realtime/connection.rs`), one per socket:
  - Upgrade config: `read_buffer_size(4 KiB)`, `write_buffer_size(8 KiB)`, `max_write_buffer_size(64 KiB)`, `max_message_size(4 KiB)`,
    `max_frame_size(4 KiB)`.
  - **Origin check** before the upgrade: allow `chrome-extension://*`, `moz-extension://*`, `safari-web-extension://*`, plus
    `YAPSPACE_ALLOWED_WS_ORIGINS`. In dev, a missing Origin is allowed (tests). Otherwise → 403.
  - **Per-IP limit**: at most 20 sockets per client IP (IPv6 keyed by /64) via `DashMap<IpKey, usize>` + an RAII guard. Global cap
    `YAPSPACE_WS_MAX_CONNECTIONS` (default 20 000) → 503.
  - Split the socket into a sender and a receiver. The **writer task** owns the sink and drains a bounded `mpsc::channel::<Message>(256)`. **If `try_send` fails because
    the queue is full → slow consumer → close 4008.** It never blocks fan-out.
  - Heartbeat: the server sends a WS `Ping` every 25 s. If no frame of any kind (pong included) arrives within 75 s → close 4004.
  - At most 5 subscriptions per connection. Each subscription is a spawned forwarder task, kept in a
    `HashMap<Uuid, JoinHandle>` and aborted on unsubscribe or disconnect.
  - Shutdown: every connection task selects on a shared `CancellationToken`. On cancel it sends close 1012 and exits. Tasks are
    tracked with `tokio_util::task::TaskTracker`, and shutdown waits up to 10 s for them.
- **Subscribe algorithm (gap-free, race-free)**:
  1. Check the discussion exists and isn't hidden (cheap PK query) → otherwise `error unknown_discussion`.
  2. `rx = hub.subscribe(id)`. **Subscribe first**, so nothing published from now on can be missed.
  3. `rows = Q3(id, after_seq, limit 501)`. If `after_seq` is absent, use `after_seq = discussion.last_seq` (live only).
  4. If `rows.len() > 500` → send `resync_required{gap_too_large}`, drop `rx`, and stop (the client reloads the newest page, then
     resubscribes with the new `after_seq`).
  5. Send each row as `message.created`. `last_sent = max(after_seq, last row seq)`. Send `subscribed{last_seq: last_sent}`.
  6. Loop on `rx.recv()`: for `Created`, skip if `ev.seq <= last_sent`, otherwise forward and set `last_sent = ev.seq`. Always forward `Updated`
     and `DiscussionUpdated`. On `RecvError::Lagged` → send `resync_required{lagged}` and end the subscription. On `Closed` → end.
- **Publishing**: domain functions call `bus.publish(...)` **after** `tx.commit()` succeeds, never inside the transaction.
  `message.updated` is published on author delete, mod remove, and restore. `discussion.updated` on lock/unlock/hide.

### G.4 Client behavior (extension `src/lib/realtime.ts`)
- One socket per open panel (the panel iframe owns it). Backoff on reconnect: `min(30 s, 1 s × 2^attempt)` with full jitter. Reset after
  the connection has been stable for 60 s. Reconnect immediately on `online` and `visibilitychange→visible` if the socket is closed.
- App-level `ping` every 25 s. If no `pong` arrives within 10 s → treat the socket as dead (it covers laptop sleep and NAT drops), close, and reconnect.
- State per discussion: `lastSeq`. On reconnect, resubscribe with `after_seq = lastSeq`.
- **Dedupe**: insert messages by `seq` into a `Map<seq, Message>` and ignore seq ≤ the current max unless it's an update. REST responses
  (your own posts) and WS echoes are merged by `seq`/`id`.
- `resync_required` → refetch the newest page (REST), replace the in-memory list (keeping any "load older" pages only if they are contiguous), and resubscribe.
- Close 1008/4029 → don't retry fast. Use a 60 s delay and show "Live updates unavailable".

### G.5 Delivery guarantees (documented)
At-least-once delivery while connected, with gap detection by contiguous `seq`, and exactly-once *display* through client dedupe. There are no client ACKs.
Recovery is pull-based through `after_seq`. A message can never be lost by a dropped notification, because it's committed before it's announced.
