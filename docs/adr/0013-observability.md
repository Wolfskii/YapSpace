# 0013 Observability

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: observability.

## Decision
JSON logs via tracing, Prometheus metrics on an internal port, and an optional Grafana profile. OpenTelemetry tracing is deferred.

## Alternatives considered
- Full OpenTelemetry with Loki and Tempo: too heavy for one host.

## Reasons
- Logs carry no URLs, bodies or IP addresses.
- Metric labels are route templates, so raw IDs never become labels.

## Consequences
- Without tracing, cross-service delays are harder to see (there is one service).

## Revisit when
- Several services or hosts exist.
