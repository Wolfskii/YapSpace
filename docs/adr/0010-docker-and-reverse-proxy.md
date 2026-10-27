# 0010 Docker and reverse proxy

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: docker and reverse proxy.

## Decision
Docker Compose on one VPS. Caddy in front for TLS. A distroless non-root image. A one-shot migrate service.

## Alternatives considered
- Kubernetes or Nomad: premature for one host.
- Nginx: manual certificate handling.
- Traefik: more moving parts for one service.

## Reasons
- Caddy issues and renews certificates itself.
- The image has no shell and runs as nonroot.

## Consequences
- One host means no high availability.
- Compose is not an orchestrator.

## Revisit when
- More than one host, or a real availability requirement.
