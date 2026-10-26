# Phase 7 · Desktop expansion

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** product validation (D-xx: a desktop use case confirmed). Each task gets a full spec when the phase starts (the planning session expands this list
the same way as Phases 1–5).
- **P7-01 · Desktop requirements** (Must): use cases (a discussion feed of followed pages, notifications, pasting a URL to open its discussion), and how the desktop app
  relates to the extension (shared account, no browser URL access).
- **P7-02 · Confirm Tauri 2** (Must): a spike app `desktop/` (Tauri 2 + Svelte 5 reusing `extension/src/components` and `src/lib/api` via an npm workspace
  package `packages/ui`). Measure the binary size, RAM, and the auto-update flow. Finalize ADR 0015.
- **P7-03 · Shared UI package extraction** (Must): move the Svelte components + api client + realtime client into `packages/yapspace-ui`, consumed by both
  the extension and desktop.
- **P7-04 · Desktop auth + discussions** (Must): token storage in the OS keychain (`tauri-plugin-stronghold` or the keyring crate), URL → discussion via the same resolve API.
- **P7-05 · Packaging & updates** (Must): Tauri bundler for Windows (MSI/NSIS), macOS (dmg, notarization), Linux (AppImage/deb), and the updater with signed manifests on GitHub
  Releases.
- **P7-06 · Cross-platform test matrix + release docs** (Must).
