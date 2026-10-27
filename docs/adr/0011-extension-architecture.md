# 0011 Extension architecture

Status: Provisional · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: extension architecture.

## Decision
WXT 0.21 with Svelte 5 and TypeScript, one codebase for Chrome and Firefox. The panel runs in an extension iframe inside a closed shadow root and is injected only on user action.

## Alternatives considered
- Render Svelte directly in a shadow root: the page can then touch the UI, and the WebSocket depends on the page.
- Side panel API: different APIs in each browser, and a different UX.

## Reasons
- An iframe is a separate origin, so page CSS and script cannot reach the panel.
- Injection on click keeps idle cost near zero.

## Consequences
- Pages with strict framing rules may block the iframe (risk R-03).
- Two browsers need two packaging flows.

## Revisit when
- The P4-02 spike shows the iframe blocked on important sites, or the side panel tests better.
