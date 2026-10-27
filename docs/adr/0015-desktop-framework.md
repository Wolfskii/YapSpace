# 0015 Desktop framework

Status: Provisional · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: desktop framework.

## Decision
Tauri 2 for a desktop client in Phase 7. It reuses yapspace-identity and the REST and WebSocket API.

## Alternatives considered
- Electron: large binaries and memory use.
- Flutter: no reuse of the Rust code.

## Reasons
- Tauri uses the system webview, so binaries are small, and Rust code is shared.

## Consequences
- Webview differences between Linux, macOS and Windows.
- Auto-update needs signed manifests.

## Revisit when
- The webview limits on Linux, or a change of product direction.
