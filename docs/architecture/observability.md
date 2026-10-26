# Observability

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## K. Observability
- **Logs:** `tracing` + `tracing-subscriber` (JSON in prod) to stdout → Docker json-file with rotation. Every request span has `request_id`, `route`,
  `method`, `status`, `latency_ms`, and `user_id` (UUIDs only; this is pseudonymous personal data and is kept 14 days through log rotation).
- **Metrics** (`metrics` + `metrics-exporter-prometheus`, served on `YAPSPACE_METRICS_ADDR`):
  `http_requests_total{route,method,status}`, `http_request_duration_seconds{route}` (histogram),
  `db_pool_connections{state=idle|active}`, `db_pool_acquire_seconds`, `db_query_errors_total`,
  `ws_connections`, `ws_subscriptions`, `ws_connects_total`, `ws_disconnects_total{code}`, `ws_slow_consumer_total`,
  `realtime_events_published_total{kind}`, `realtime_deliveries_total`, `realtime_lagged_total`, `realtime_resync_total{reason}`,
  `message_post_to_publish_seconds`, `messages_created_total`, `ratelimit_rejections_total{bucket}`, `auth_login_failures_total`,
  `jobs_runs_total{job,result}`, `build_info{version,git_sha}`.
- **Postgres:** `pg_stat_statements` + postgres-exporter (monitoring profile). **Host:** node-exporter.
- **Tracing/OTel:** deferred. `tracing` spans make an `opentelemetry-otlp` layer a one-file addition when there are multiple services.
- **Alerts** (Grafana alerting → email/Discord webhook; initial defaults, **tune after 2 weeks of production data**):
  5xx ratio > 2% for 5 min · p95 latency > 500 ms for 10 min (excluding WS) · `db_pool_acquire_seconds` p95 > 100 ms for 5 min · disk > 80% ·
  backup heartbeat missing > 26 h · restore test failed · `ws_connections` dropped > 50% within 5 min · container restart count > 3 in 15 min ·
  `ratelimit_rejections_total` 10× above the 7-day baseline.
- **External:** an uptime monitor on `https://<domain>/readyz` every 1 min (free tier of any uptime service).
