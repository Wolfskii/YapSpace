# Phase 3 · Real-time discussions

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** the Phase 2 gate has PASSED. **Parallelizable:** P4-01..P4-03 can start once P3-01 is DONE (the protocol types are fixed).
**Exit criteria:** two clients see each other's messages live, a reconnect with `after_seq` recovers what was missed, every WS lifecycle test passes, and memory/latency
measurements are recorded in `capacity.md`.

### P3-01 · Protocol types
**Priority:** Must · **Deps:** P2-22 · **Spec:** §G.2
**Deliverables:** `realtime/protocol.rs`: `#[serde(tag = "type")] enum ClientFrame { #[serde(rename="hello")] Hello{protocol:u8, client:Option<String>,
token:Option<String>}, Subscribe{discussion_id:Uuid, after_seq:Option<i64>}, Unsubscribe{discussion_id:Uuid}, Ping{nonce:Option<u64>} }`
and `enum ServerFrame { Welcome{…}, HelloOk{user_id:Option<Uuid>}, Subscribed{…}, #[serde(rename="message.created")] MessageCreated{discussion_id, seq,
message: MessageDto}, #[serde(rename="message.updated")] MessageUpdated{…}, #[serde(rename="discussion.updated")] DiscussionUpdated{…},
ResyncRequired{discussion_id, reason}, Pong{nonce}, Error{code, detail} }` with renames to snake_case per §G.2. Also `docs/architecture/realtime.md` gets a
JSON example for each frame, and a TS mirror `extension/src/lib/realtime-protocol.ts` is written by hand (WS frames aren't in OpenAPI, so keep the mirror small).
**Tests:** a serde round-trip for each frame. `{"type":"bogus"}` → error.

### P3-02 · Hub
**Priority:** Must · **Deps:** P3-01 · **Spec:** §G.3
**Deliverables:** `realtime/hub.rs`:
```rust
pub struct Event { pub discussion_id: Uuid, pub kind: EventKind, pub seq: i64, pub payload: Utf8Bytes }
#[derive(Clone, Default)] pub struct Hub { inner: Arc<DashMap<Uuid, broadcast::Sender<Arc<Event>>>> }
impl Hub {
    pub fn subscribe(&self, id: Uuid) -> broadcast::Receiver<Arc<Event>>;          // entry().or_insert_with(|| broadcast::channel(1024).0).subscribe()
    pub fn publish(&self, ev: Event) -> usize;                                       // if let Some(tx) = get(id) { tx.send(Arc::new(ev)).unwrap_or(0) } else { 0 }
    pub fn release(&self, id: Uuid);                                                 // remove_if(|_, tx| tx.receiver_count() == 0)
    pub fn channel_count(&self) -> usize;
}
```
`realtime/bus.rs`: `#[async_trait]`-free trait using `fn publish(&self, ev: Event) -> impl Future<Output=()> + Send` (or a boxed future), `LocalBus(Hub)`.
AppState gets `hub: Hub, bus: Arc<LocalBus>` (concrete type now; it becomes a generic or enum in P6-02).
**Tests:** publish with no subscribers → 0. 2 subscribers both receive. Release after both are dropped removes the entry. Lagged when 1100 events go to a slow receiver.

### P3-03 · WebSocket endpoint & connection task
**Priority:** Must · **Deps:** P3-02, P2-18 · **Spec:** §G.3
**Deliverables:** `api/realtime.rs` route `GET /v1/realtime` (outside the timeout layer): extract `WebSocketUpgrade`, `ClientIp`, the `Origin`
header → origin check → per-IP and global guards (`ConnectionGuard` RAII decrementing `DashMap<IpKey, usize>` and an `AtomicUsize`) → `ws.read_buffer_size(4096)
.write_buffer_size(8192).max_write_buffer_size(65536).max_message_size(4096).max_frame_size(4096).on_upgrade(move |socket| connection::run(…))`.
`realtime/connection.rs::run(socket, state, guard, ip)`:
```text
let (sink, stream) = socket.split();
let (out_tx, out_rx) = mpsc::channel::<Message>(256);
spawn writer: forward out_rx → sink; on error break.
send Welcome.
loop select! {
  _ = shutdown.cancelled() => { send Close(1012) via out_tx (best effort); break }
  _ = heartbeat.tick() (25 s) => { if last_inbound.elapsed() > 75 s { close 4004; break } ; out_tx.try_send(Ping) }
  msg = stream.next() => match msg {
     Text(t) => { rate-check frames (20/10 s); parse ClientFrame (error → Error{bad_frame}); handle Hello/Subscribe/Unsubscribe/Ping }
     Pong/Ping => last_inbound = now
     Close/None/Err => break
     Binary => close 4000
  }
}
abort every subscription task; hub.release(each id); drop guard.
```
Helper `fn send(out_tx, frame) -> Result<(), SlowConsumer>` uses `try_send`. If the queue is full → `ws_slow_consumer_total` += 1 → close 4008 → break.
Hello with a token: validate with Q6 (no touch) → `HelloOk{user_id}`. An invalid token → `Error{unauthorized}`, and the connection stays anonymous.
Register the connection task in `state.tasks` (the TaskTracker). `TestApp::ws_connect()` helper.
**Tests:** connect → Welcome. A bad origin → 403 (with `YAPSPACE_ENV=prod` in the test config). The 21st connection from the same IP → 4029/503 (document which.
**Use HTTP 429 before the upgrade**). A garbage frame → Error bad_frame. A 5 KiB frame → close 1009. A ping → pong with the nonce.

### P3-04 · Subscribe with gap-free catch-up
**Priority:** Must · **Deps:** P3-03 · **Spec:** §G.3 subscribe algorithm
**Deliverables:** `handle_subscribe` exactly per steps 1–6. The subscription forwarder is a spawned task holding `rx` and a clone of `out_tx`. Max 5 subscriptions per connection
→ `Error{too_many_subscriptions}`. Re-subscribing to the same id replaces the old task. Metrics `ws_subscriptions`, `realtime_resync_total{reason}`,
`realtime_lagged_total`, `realtime_deliveries_total`.
**Tests:** (1) post 3 messages, subscribe with after_seq=1 → receive seq 2,3, then Subscribed{last_seq:3}. (2) Subscribe, then post → message.created arrives.
(3) A gap of 600 → resync_required{gap_too_large}. (4) The race: spawn 20 posters concurrently with the subscribe call → the client sees every seq exactly once, in
increasing order with no gaps (assert against the DB). (5) An unknown discussion id → Error unknown_discussion.

### P3-05 · Publish from the domain
**Priority:** Must · **Deps:** P3-04
**Deliverables:** replace the `// P3-05` markers: after commit in `post` → `bus.publish(Event{kind: Created, seq, payload: serde_json::to_string(&ServerFrame::MessageCreated{…})})`.
After an author delete → `MessageUpdated`. Metric `message_post_to_publish_seconds` (from the start of the handler to after publish).
**Tests:** a WS subscriber sees created and updated events. Publishing never happens when the tx fails (a forced constraint failure → no event).

### P3-06 · Graceful shutdown for sockets
**Priority:** Must · **Deps:** P3-05
**Deliverables:** on SIGTERM: readiness flips to 503 immediately (an `AtomicBool draining`), the HTTP listener stops, every socket gets close 1012, the TaskTracker waits ≤
`shutdown_grace`, then the pool closes. Document it in `realtime.md`.
**Tests:** an integration test cancels the app's token → the client receives close code 1012 within 1 s.

### P3-07 · Authenticated hello + per-user connection cap
**Priority:** Should · **Deps:** P3-03
**Deliverables:** an authenticated connection is counted per user (max 10 sockets per user → `Error{rate_limited}` + close 4029). The user id goes on the connection span.
**Tests:** an 11th authenticated socket for the same user is closed with 4029.

### P3-08 · Realtime robustness test suite
**Priority:** Must · **Deps:** P3-06
**Deliverables:** `tests/realtime.rs` completed: a slow consumer (a client that stops reading. Send enough events to fill the 256 queue + the TCP buffers; use 5k
events of 1.5 KB bodies) → close 4008. Heartbeat timeout (configurable intervals for tests via `Config` fields `ws_heartbeat`, `ws_idle_timeout`) →
4004. Reconnect flow: subscribe → disconnect → post 5 → reconnect with after_seq → receive exactly those 5.
**Acceptance:** the whole suite runs in < 30 s and isn't flaky (run it 10× in a loop locally: `for i in $(seq 10); do cargo test --test realtime || break; done`).

### P3-09 · Load scripts + first measurements
**Priority:** Should · **Deps:** P3-08 · **Spec:** §L.5
**Deliverables:** `loadtest/{browse.js, chat-viral.js, reconnect-storm.js, soak.js, README.md}`, `yapspace admin seed-load` (dev only),
`task loadtest:viral VIEWERS=1000`, `task loadtest:browse VUS=200`. Run them locally against `task api:run --release` + the dev DB. Record in
`docs/architecture/capacity.md` → "Measured (dev machine)": the RSS per 1k idle sockets, delivery p50/p95/p99 for 1k viewers at 2 msg/s, and the HTTP
p95 at 200 VUs. Note the hardware.
**Acceptance:** the numbers are recorded, with any surprises logged as risks.
**Notes:** k6 via `docker run --rm -i --network host grafana/k6 run - < loadtest/browse.js`. On Windows Docker Desktop, use `host.docker.internal:8080` as the target
(the scripts read `__ENV.BASE_URL`).
