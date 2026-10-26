# Deployment & operations

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## J. Deployment & operations

### J.1 Containers
| Service | Image | Why it's separate |
|---|---|---|
| `caddy` | `caddy:2-alpine` (pin a minor, e.g. `2.10-alpine`) | TLS/ACME, the only public ports |
| `migrate` | the app image, `command: ["migrate"]` | one-shot, runs before `api` |
| `api` | `ghcr.io/<owner>/yapspace:<tag>` | the app (HTTP + WS + in-process jobs) |
| `postgres` | `postgres:18-alpine` | state |
| `backup` | `deploy/backup/Dockerfile` (postgres:18-alpine + restic + supercronic) | scheduled encrypted off-site dumps |
| `prometheus`, `grafana`, `node-exporter`, `postgres-exporter` | profile `monitoring` (off by default) | optional observability |
The background **worker stays in-process** (purge jobs are tiny). Split it out only when jobs measurably affect request latency.

### J.2 Dockerfile (`Dockerfile` at the repo root)
```dockerfile
# syntax=docker/dockerfile:1.7
FROM rust:1.98-slim-trixie AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --locked -p yapspace-server --bin yapspace

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=builder /app/target/release/yapspace /app/yapspace
USER nonroot:nonroot
EXPOSE 8080 9090
ENTRYPOINT ["/app/yapspace"]
CMD ["serve"]
```
Notes: no TLS libraries are needed (the DB is on a private network and the server makes no outbound HTTPS), so there are no OpenSSL/aws-lc build deps. Migrations are embedded
(`sqlx::migrate!`). Healthcheck = `["/app/yapspace", "healthcheck"]` (distroless has no curl). If the `rust:1.98-slim-trixie` or
`distroless/cc-debian13` tag doesn't exist, use `rust:1.98-slim-bookworm` + `distroless/cc-debian12` (the glibc must match: builder ≤ runtime).
Image metadata: OCI labels with `org.opencontainers.image.revision` set via `--build-arg GIT_SHA`. `.dockerignore` excludes `target/`,
`extension/`, `.git/`, `docs/`, `.env*`.

### J.3 Production compose (`deploy/compose.yaml`, illustrative; P5-09 finalizes it)
```yaml
name: yapspace
x-app: &app
  image: ${YAPSPACE_IMAGE:-ghcr.io/OWNER/yapspace}:${YAPSPACE_IMAGE_TAG:?set YAPSPACE_IMAGE_TAG}
  env_file: .env
  environment:
    YAPSPACE_DATABASE_URL_FILE: /run/secrets/database_url
  secrets: [database_url]
  logging: { driver: json-file, options: { max-size: "10m", max-file: "5" } }

services:
  caddy:
    image: caddy:2.10-alpine
    restart: unless-stopped
    ports: ["80:80", "443:443", "443:443/udp"]
    environment: { YAPSPACE_DOMAIN: "${YAPSPACE_DOMAIN:?}", ACME_EMAIL: "${ACME_EMAIL:?}" }
    volumes: ["./Caddyfile:/etc/caddy/Caddyfile:ro", "caddy_data:/data", "caddy_config:/config"]
    depends_on: { api: { condition: service_healthy } }
    networks: [edge]
  migrate:
    <<: *app
    command: ["migrate"]
    restart: "no"
    depends_on: { postgres: { condition: service_healthy } }
    networks: [backend]
  api:
    <<: *app
    command: ["serve"]
    restart: unless-stopped
    depends_on: { migrate: { condition: service_completed_successfully } }
    healthcheck: { test: ["CMD", "/app/yapspace", "healthcheck"], interval: 10s, timeout: 3s, retries: 3, start_period: 15s }
    stop_grace_period: 30s
    deploy: { resources: { limits: { cpus: "2.0", memory: 1g } } }
    networks: [edge, backend]
  postgres:
    image: postgres:18-alpine
    restart: unless-stopped
    command: ["postgres", "-c", "shared_preload_libraries=pg_stat_statements", "-c", "max_connections=100",
              "-c", "shared_buffers=512MB", "-c", "log_min_duration_statement=500ms"]
    environment: { POSTGRES_USER: yapspace, POSTGRES_DB: yapspace, POSTGRES_PASSWORD_FILE: /run/secrets/postgres_password }
    secrets: [postgres_password]
    volumes: ["pgdata:/var/lib/postgresql", "./postgres/init:/docker-entrypoint-initdb.d:ro"]   # PG18: mount the parent dir
    healthcheck: { test: ["CMD-SHELL", "pg_isready -U yapspace -d yapspace"], interval: 5s, timeout: 3s, retries: 10 }
    shm_size: 256mb
    stop_grace_period: 60s
    networks: [backend]
  backup:
    build: ./backup
    restart: unless-stopped
    env_file: backup.env          # RESTIC_REPOSITORY, AWS_*, HEALTHCHECK_URL (gitignored)
    secrets: [postgres_password, restic_password]
    depends_on: { postgres: { condition: service_healthy } }
    networks: [backend, egress]

networks:
  edge: {}
  backend: { internal: true }
  egress: {}
volumes: { pgdata: {}, caddy_data: {}, caddy_config: {} }
secrets:
  database_url:      { file: ./secrets/database_url }
  postgres_password: { file: ./secrets/postgres_password }
  restic_password:   { file: ./secrets/restic_password }
```
`deploy/postgres/init/01-extensions.sql`: `CREATE EXTENSION IF NOT EXISTS pg_stat_statements;`.
**A healthy container is not a healthy application:** the Docker healthcheck (`/healthz`) only proves the process answers. `/readyz` (DB reachable,
not draining) is what the external uptime monitor checks, together with the synthetic check "resolve + list messages on a known discussion".

### J.4 Caddyfile (`deploy/Caddyfile`)
```caddyfile
{
	email {$ACME_EMAIL}
}
{$YAPSPACE_DOMAIN} {
	encode zstd gzip
	request_body {
		max_size 64KB
	}
	header {
		Strict-Transport-Security "max-age=31536000; includeSubDomains"
		X-Content-Type-Options "nosniff"
		Referrer-Policy "no-referrer"
		-Server
	}
	@internal path /metrics* /docs* /openapi.json
	respond @internal 404
	reverse_proxy api:8080 {
		transport http {
			read_timeout 0
			write_timeout 0
		}
	}
}
```
Caddy redirects HTTP→HTTPS automatically, and WebSockets pass through `reverse_proxy` with no extra config. Access logging is **off** (privacy). The app enforces its
own HTTP timeouts, and the zero proxy timeouts keep long-lived WebSockets open. Caddy replaces any client-supplied `X-Forwarded-For` with the real peer, so the app
trusts XFF only from the `edge` network CIDR (`YAPSPACE_TRUSTED_PROXIES=172.16.0.0/12`, or pin the compose subnet).
Hostname validation: Caddy only serves `{$YAPSPACE_DOMAIN}`. Other Host headers get no certificate or route.

### J.5 Configuration (env, `crates/yapspace-server/src/config.rs`)
| Var | Default | Notes |
|---|---|---|
| `YAPSPACE_ENV` | `dev` | `dev` \| `prod` (prod: no /docs, JSON logs, a missing Origin is rejected) |
| `YAPSPACE_DATABASE_URL` / `_FILE` | – | required (one of the two) |
| `YAPSPACE_DB_MAX_CONNECTIONS` | 20 | pool size |
| `YAPSPACE_BIND_ADDR` | `0.0.0.0:8080` | |
| `YAPSPACE_METRICS_ADDR` | `0.0.0.0:9090` | internal only, never published |
| `YAPSPACE_LOG_FORMAT` | `pretty` (dev) / `json` (prod) | plus `RUST_LOG` (default `info,yapspace=debug` in dev) |
| `YAPSPACE_TRUSTED_PROXIES` | empty | comma-separated CIDRs |
| `YAPSPACE_ALLOWED_WS_ORIGINS` | empty | extra origins beyond the extension schemes |
| `YAPSPACE_WS_MAX_CONNECTIONS` | 20000 | |
| `YAPSPACE_REGISTRATION_OPEN` | true | kill switch |
| `YAPSPACE_EXTRA_BLOCKED_HOSTS` | empty | comma-separated |
| `YAPSPACE_SHUTDOWN_GRACE_SECS` | 20 | |
Every `*_FILE` variant reads and trims the file contents. Config validation fails fast at startup with a clear message.

### J.6 Operations
- **Deploy / upgrade:** `task deploy TAG=v0.3.0` → SSH: `cd /opt/yapspace && ./backup-now.sh && sed -i "s/^YAPSPACE_IMAGE_TAG=.*/YAPSPACE_IMAGE_TAG=v0.3.0/" .env && docker compose pull && docker compose up -d && ./smoke.sh`.
  WS clients get 1012 and reconnect. Downtime is about 2–5 s on a single instance (acceptable for the MVP and documented).
- **Rollback:** set the previous tag and run `docker compose up -d`. This is safe because every migration follows expand/contract (§M.4). If a contract migration has
  already run, roll forward with a fix, or restore from backup (runbook).
- **Backups:** daily at 03:15 UTC: `pg_dump -Fc | restic backup --stdin --stdin-filename yapspace.dump` → S3-compatible storage (D-02), restic
  AES-256 encryption with the password from a secret. Retention `--keep-daily 7 --keep-weekly 4 --keep-monthly 12`. A heartbeat ping to an external dead-man
  switch (e.g. healthchecks.io) after success. Weekly `restic check --read-data-subset=5%`.
- **Restore test:** weekly cron + `task ops:restore-test` restores the latest snapshot into a temp DB `yapspace_restore_test`, runs sanity queries
  (row counts > 0, `max(seq)` matches `last_seq` for 100 random discussions), and drops it. The result is pushed as a metric/heartbeat.
- **Secrets rotation:** DB password (ALTER ROLE + update both secret files + `docker compose up -d`), restic password (`restic key add/remove`).
  Session tokens can be mass-revoked with `yapspace admin revoke-all-sessions`.
- **Storage exhaustion:** alert at 80% disk. Postgres needs ~2× headroom for VACUUM FULL/reindex. Docker log rotation is configured. Run `docker system
  prune` for old images in the deploy script (keep the last 3 tags).
- **What a single VPS still leaves to you:** OS patching (unattended-upgrades), SSH hardening (keys only, fail2ban), firewall (ufw: 22/80/443),
  Docker engine updates, disk monitoring, and DR (rebuild from backup onto a new VPS in under 2 h with the runbook). **Compose is not HA.** A host failure means
  downtime until the restore.
