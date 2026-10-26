---
name: next-task
description: Continue YapSpace implementation. Use when the user says "continue yapspace", "next task", or "work on the next YapSpace task". Follows the docs/plan/README.md resume protocol.
---

# Next YapSpace task

1. Read `AGENTS.md`, `docs/plan/README.md` (status board and resume protocol), and the last 3 entries of `docs/plan/HANDOFF.md`.
2. Run `git status` and `git log --oneline -10`. A dirty tree with no `IN_PROGRESS` handoff means stop and ask the user.
3. Pick the task per the resume protocol: an `IN_PROGRESS` or `REOPENED` task first, otherwise the first `TODO` task whose deps are DONE.
4. Mark it `IN_PROGRESS` on the board. Read its spec in `docs/plan/phase-N.md` and every linked architecture doc.
5. Implement step by step, tick checkboxes, run `task check` (or the subset that exists), then mark `DONE` or `IN_REVIEW`.
6. Append a HANDOFF entry and commit with conventional messages. No AI attribution in commits or docs.
