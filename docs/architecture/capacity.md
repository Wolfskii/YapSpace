# Capacity & cost model

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## L. Capacity & cost model

**Everything here is an estimate from the stated assumptions, not a guarantee.** P3-09 and P5-12 replace the estimates with measurements
(a "Measured" table is added to this doc).

### L.1 Workload assumptions
- R = registered users. DAU = 20% of R. **C = peak concurrent users with the extension active** (the scenario table).
- Panel-open share of C = **25%** → open sockets `W = 0.25·C`. The design target is `W = C` (a 4× headroom case).
- Each open panel navigates every 2 min → resolve + history = 2 requests. Plus 10% "load older" and miscellaneous → **≈ 0.02 req/s per open panel**.
- Writes: **1 message per open panel per 15 min** → 0.0011 msg/s per open panel.
- Average concurrency over a day ≈ 40% of peak. Messages/day = 0.4·C·0.25·24 h·4 msg/h = **0.96·C**.
- Typical fan-out: a median discussion has 1–3 viewers. **Skew case:** one viral discussion holds 30% of open panels and gets 30% of writes.
- Message on the wire ≈ 450 B JSON (WS frame) and ≈ 25 KB per history page of 50 (≈ 6 KB gzipped).

### L.2 Per scenario (peak)
| | MVP | Early | Established | Large |
|---|---:|---:|---:|---:|
| Registered / concurrent C | 1k / 100 | 10k / 1k | 100k / 10k | 1M / 100k |
| WS sockets expected (design) | 25 (100) | 250 (1k) | 2.5k (10k) | 25k (100k) |
| HTTP req/s | 0.5 | 5 | 50 | 500 |
| Message writes/s | 0.03 | 0.28 | 2.8 | 28 |
| Messages/day → DB growth/yr | ~100 → <0.1 GB | ~1k → 0.3 GB | ~10k → 2.6 GB | ~100k → 26 GB |
| Viral discussion viewers | 8 | 75 | 750 | 7,500 |
| Viral deliveries/s (30% of writes × viewers) | ~0.1 | ~6 | ~630 | ~63,000 |
| Viral egress | tiny | ~3 KB/s | ~0.3 MB/s | ~28 MB/s (≈230 Mbit/s) |
| DB pool (API) | 5 | 10 | 20 | 40 per instance (+ replicas) |
| Suggested host | 2 vCPU / 4 GB | 2 vCPU / 4 GB | 4 vCPU / 8 GB (+ maybe separate DB host) | 2–4 API hosts (8 vCPU/16 GB each) + dedicated DB (16 vCPU/64 GB, NVMe) + replica |
| Backups (compressed, 1 yr retained) | < 1 GB | < 2 GB | ~10 GB | ~100 GB |

**Memory per socket** (estimated, measure in P3-09): the tokio task + split halves + 4 KiB read buffer + up to 8–64 KiB write buffer + a 256-slot
mpsc + forwarder tasks ≈ **20–40 KB typical**. 10k sockets ≈ 0.2–0.4 GB. 100k ≈ 2–4 GB, which is why the large scenario uses multiple instances.
Broadcast payloads are shared through `Utf8Bytes`, so the cost per extra viewer is a refcount plus the socket write.

**The bottlenecks, in the order we expect to hit them:** (1) a viral-discussion fan-out egress/CPU → slow mode (P6-05) + multiple instances; (2) the history
read path on popular pages → an in-process moka cache of the newest page per discussion (invalidated on publish); (3) Postgres write IOPS/WAL
at around 100+ writes/s with fsync → a better disk first, then tune `synchronous_commit` per transaction for non-critical writes;
(4) one VPS's network/CPU for 25k+ sockets → a second instance + `PgNotifyBus`.

### L.3 Costs (order of magnitude; **verify current provider prices before deciding**, as they change)
- MVP/Early: one budget-provider VPS with 2–4 vCPU / 4–8 GB (e.g. Hetzner, OVH, Scaleway class) + ~100 GB S3-compatible backup storage
  + a domain → typically **low tens of EUR/month in total**.
- Established: a 4–8 vCPU / 16 GB VPS, or a split API host + DB host → **tens of EUR/month to about 100 EUR/month**.
- Large: several hosts + a DB with a replica + DDoS protection → **hundreds of EUR/month**. Revisit the architecture before this point (§L.4).

### L.4 Scaling triggers (measured, not user-count driven)
| Introduce | When (any of these is sustained ≥ 1 week, or projected in < 1 month) |
|---|---|
| Bigger VPS (vertical first) | CPU > 60% at peak, RAM > 75%, or p95 latency > 300 ms with no slow query to blame |
| moka read cache (P6-04) | resolve/history is > 50% of DB time in `pg_stat_statements`, or DB CPU > 50% |
| Separate DB host | DB and API compete (steal/IO wait > 10%), or DB RAM needs to exceed half the host |
| 2nd API instance + `PgNotifyBus` (P6-02) | WS sockets > 15k, or API CPU > 60% after vertical scaling, or a zero-downtime deploy requirement |
| NATS instead of NOTIFY | NOTIFY > ~1–2k events/s, or the LISTEN connection lagging (`pg_stat_activity` wait events), or more than 4 instances |
| Redis | only if limits or caches must be shared across instances *and* in-process approximations cause measurable abuse/staleness |
| Dedicated worker | job runtime > 10% of a core, or job latency affects p95 |
| Read replica | read QPS saturates the primary after caching, or analytics/search queries hurt OLTP |
| Partition `messages` (by hash of discussion_id or by month) | messages table > ~500 GB or > ~2 billion rows, or VACUUM can't keep up |
| Search engine (Meilisearch/OpenSearch) | Postgres FTS p95 > 500 ms, or relevance requirements FTS can't meet |
| Kubernetes | probably never for this team size. Consider Nomad/K8s only with more than 5 hosts and several services |

### L.5 Load testing (k6, `loadtest/`, run with the `grafana/k6` Docker image)
- `browse.js`: VUs = open panels; each one does resolve → history → idle 120 s → loop. Uses a pool of 10k synthetic URLs with a Zipf distribution.
- `chat-viral.js`: N WS viewers subscribed to one discussion + M posting users at 1 msg/15 s each. **Delivery latency** = receive time −
  the `sent_at` that the poster embeds in the body. Report p50/p95/p99.
- `reconnect-storm.js`: 5k sockets connect, the server restarts (close 1012), and we measure reconnect time and catch-up correctness (no gaps by seq).
- `soak.js`: the Established-scenario mix for 2 h. Watch RSS, open FDs, pool waits, and the ws_connections gauge (looking for leaks).
- Seed data: `yapspace admin seed-load --users 10000 --discussions 50000 --messages 2000000` (dev/staging only; refuses when `YAPSPACE_ENV=prod`).
- Pass criteria for the MVP (P5-12): at 2.5k sockets + 50 req/s + 3 writes/s on a 4 vCPU/8 GB box: HTTP p95 < 150 ms, delivery p95 < 250 ms,
  error rate < 0.1%, no memory growth over 2 h. A viral test with 2k viewers in one discussion at 2 msg/s keeps delivery p95 < 500 ms.
