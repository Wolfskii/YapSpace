# Data model

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## D. Data model

### D.1 Conventions
- **IDs:** `uuid`. The app generates them as **UUIDv7** (`uuid::Uuid::now_v7()`), so they are time-ordered, keep B-tree inserts mostly
  append-only, can't be enumerated like integers, and can be generated offline by clients. The one exception is `audit_events.id`, a `bigint identity`
  (internal only). PG18 has `uuidv7()`, but we generate IDs in Rust so tests and app code control them.
- **Timestamps:** `timestamptz`, always UTC. The API sends them as RFC 3339 with milliseconds (`2026-10-26T12:00:00.123Z`).
- **Enums:** `text` + `CHECK` constraints, not PG enum types, because they are easier to evolve in migrations.
- **Message order:** per-discussion gapless `seq bigint`, assigned in the same transaction as the insert by bumping
  `discussions.last_seq`. `seq` is the cursor, the ordering key, and the WebSocket resume point. We never order by timestamp.
- **Pagination:** keyset only (`seq < $cursor ORDER BY seq DESC LIMIT n`). No OFFSET.
- **Deletion:** users are **soft-deleted and anonymized**. Messages become **tombstones** (`status` changes and the body is erased) so that `seq`
  stays gapless and reply references stay valid. Physical deletes happen only through retention jobs.
- **Migrations:** the `sqlx migrate` format, files named `NNNN_description.sql` in `crates/yapspace-server/migrations/`, embedded with
  `sqlx::migrate!()`. **A migration is immutable once committed to `develop`.** Changes go into new files. Use expand→contract for breaking
  changes (§M.4).
- **Polymorphic targets** (`reports.target_id`, `moderation_actions.target_id`) have no FK. The domain layer validates the target
  exists in the same transaction. This is documented and tested.

### D.2 Migration `0001_init.sql` (MVP schema, copy verbatim)

```sql
-- Users ---------------------------------------------------------------
CREATE TABLE users (
    id              uuid PRIMARY KEY,
    username        text NOT NULL,
    role            text NOT NULL DEFAULT 'user'   CHECK (role IN ('user', 'moderator', 'admin')),
    status          text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'deleted')),
    suspended_until timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    deleted_at      timestamptz,
    CONSTRAINT users_username_format CHECK (username ~ '^[a-z0-9_]{3,24}$')
);
CREATE UNIQUE INDEX users_username_key ON users (username);

-- Password credential + recovery code (separate table so other auth methods can be added later)
CREATE TABLE password_credentials (
    user_id            uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    password_hash      text  NOT NULL,          -- PHC string, argon2id
    recovery_code_hash bytea NOT NULL,          -- sha256 of the normalized recovery code
    updated_at         timestamptz NOT NULL DEFAULT now()
);

-- Sessions (opaque bearer tokens; only SHA-256 of token stored) --------
CREATE TABLE sessions (
    id           uuid PRIMARY KEY,
    user_id      uuid  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash   bytea NOT NULL,
    client_label text CHECK (char_length(client_label) <= 100),
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    revoked_at   timestamptz
);
CREATE UNIQUE INDEX sessions_token_hash_key ON sessions (token_hash);
CREATE INDEX sessions_user_id_idx    ON sessions (user_id);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

-- Discussions -----------------------------------------------------------
CREATE TABLE discussions (
    id              uuid PRIMARY KEY,
    display_url     text NOT NULL CHECK (char_length(display_url) <= 2048),
    site            text NOT NULL CHECK (char_length(site) <= 253),
    title           text CHECK (char_length(title) <= 300),
    status          text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'locked', 'hidden')),
    merged_into_id  uuid REFERENCES discussions (id),
    last_seq        bigint  NOT NULL DEFAULT 0,
    message_count   integer NOT NULL DEFAULT 0,
    created_by      uuid REFERENCES users (id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    last_message_at timestamptz,
    CONSTRAINT discussions_not_merged_into_self CHECK (merged_into_id IS NULL OR merged_into_id <> id)
);

-- Identity keys → discussion (many keys may point to one discussion) ----
CREATE TABLE discussion_identities (
    key_hash      bytea PRIMARY KEY,            -- sha256(key)
    key           text NOT NULL CHECK (char_length(key) <= 2100),
    discussion_id uuid NOT NULL REFERENCES discussions (id) ON DELETE CASCADE,
    ruleset       smallint NOT NULL,            -- canonicalization ruleset version (1 for MVP)
    kind          text NOT NULL CHECK (kind IN ('url', 'canonical', 'adapter', 'legacy', 'merge')),
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX discussion_identities_discussion_idx ON discussion_identities (discussion_id);

-- Messages ----------------------------------------------------------------
CREATE TABLE messages (
    id                uuid PRIMARY KEY,
    discussion_id     uuid   NOT NULL REFERENCES discussions (id),
    seq               bigint NOT NULL,
    author_id         uuid   NOT NULL REFERENCES users (id),
    parent_id         uuid,
    body              text   NOT NULL CHECK (char_length(body) <= 2000),
    status            text   NOT NULL DEFAULT 'visible' CHECK (status IN ('visible', 'deleted', 'removed')),
    client_msg_id     uuid   NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    edited_at         timestamptz,
    status_changed_at timestamptz,
    CONSTRAINT messages_discussion_seq_key   UNIQUE (discussion_id, seq),
    CONSTRAINT messages_discussion_id_id_key UNIQUE (discussion_id, id),          -- target of the parent FK
    CONSTRAINT messages_author_client_key    UNIQUE (author_id, client_msg_id),   -- idempotent posting
    CONSTRAINT messages_parent_same_discussion
        FOREIGN KEY (discussion_id, parent_id) REFERENCES messages (discussion_id, id),
    CONSTRAINT messages_no_self_parent CHECK (parent_id IS NULL OR parent_id <> id)
);
CREATE INDEX messages_author_created_idx ON messages (author_id, created_at DESC);
CREATE INDEX messages_parent_idx ON messages (parent_id) WHERE parent_id IS NOT NULL;

-- Reports -------------------------------------------------------------------
CREATE TABLE reports (
    id          uuid PRIMARY KEY,
    reporter_id uuid NOT NULL REFERENCES users (id),
    target_type text NOT NULL CHECK (target_type IN ('message', 'user', 'discussion')),
    target_id   uuid NOT NULL,
    reason      text NOT NULL CHECK (reason IN ('spam', 'harassment', 'hate', 'sexual', 'violence',
                                                'self_harm', 'illegal', 'privacy', 'wrong_discussion', 'other')),
    details     text CHECK (char_length(details) <= 1000),
    status      text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'actioned', 'dismissed')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    resolved_by uuid REFERENCES users (id),
    resolved_at timestamptz,
    CONSTRAINT reports_one_per_reporter_target UNIQUE (reporter_id, target_type, target_id)
);
CREATE INDEX reports_open_queue_idx ON reports (created_at) WHERE status = 'open';
CREATE INDEX reports_target_idx     ON reports (target_type, target_id);

-- Moderation actions (also the moderation audit trail) --------------------
CREATE TABLE moderation_actions (
    id           uuid PRIMARY KEY,
    moderator_id uuid NOT NULL REFERENCES users (id),
    action       text NOT NULL CHECK (action IN ('remove_message', 'restore_message', 'suspend_user', 'unsuspend_user',
                                                 'lock_discussion', 'unlock_discussion', 'hide_discussion',
                                                 'unhide_discussion', 'dismiss_report', 'merge_discussion')),
    target_type  text NOT NULL CHECK (target_type IN ('message', 'user', 'discussion', 'report')),
    target_id    uuid NOT NULL,
    report_id    uuid REFERENCES reports (id),
    reason       text CHECK (char_length(reason) <= 1000),
    expires_at   timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX moderation_actions_target_idx ON moderation_actions (target_type, target_id, created_at DESC);

-- Security-relevant audit events (role changes, account deletion, password changes) ---
CREATE TABLE audit_events (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_id    uuid REFERENCES users (id),
    action      text NOT NULL CHECK (char_length(action) <= 64),
    target_type text,
    target_id   uuid,
    metadata    jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_events_created_idx ON audit_events (created_at);
```

Phase 5 adds migration `0002_user_blocks.sql`:
```sql
CREATE TABLE user_blocks (
    blocker_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    blocked_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CONSTRAINT user_blocks_not_self CHECK (blocker_id <> blocked_id)
);
```

**Tables deliberately left out of the MVP:** reactions (Phase 6: `message_reactions(message_id, user_id, emoji)` PK on all three, which
blocks duplicates), follows, notifications, user_preferences (no prefs needed yet; settings that are purely client-side live in `storage.local`), and
a jobs table (MVP background work is periodic and idempotent, so it doesn't need a durable queue).

**Integrity guarantees:**
- *Duplicate discussions:* `discussion_identities.key_hash` is the PK, so two creators of the same key race on the unique key and one of them wins (§D.3 Q4).
- *Invalid replies:* the composite FK `(discussion_id, parent_id) → messages(discussion_id, id)` makes it impossible to reply across
  discussions, and the CHECK blocks self-reference. A parent always exists because messages are never physically deleted (only tombstoned).
- *Duplicate posts on retry:* `UNIQUE (author_id, client_msg_id)`.
- *Duplicate reports:* `UNIQUE (reporter_id, target_type, target_id)`.
- *Gapless order:* `UNIQUE (discussion_id, seq)` + the row lock on `discussions` during the increment.

### D.3 Hot queries (with the index each one uses)

**Q1 Resolve identity → discussion** (PK lookup on `discussion_identities`). Run it once per candidate key, in priority order:
```sql
SELECT d.id, d.display_url, d.site, d.title, d.status, d.merged_into_id, d.last_seq, d.message_count,
       d.created_at, d.last_message_at
FROM discussion_identities di JOIN discussions d ON d.id = di.discussion_id
WHERE di.key_hash = $1;
```
If `merged_into_id` is set, load that discussion instead (one hop; merges always point at the final target).

**Q2 Newest page / older page** (uses `messages_discussion_seq_key`; `$2 = i64::MAX` for the newest page):
```sql
SELECT m.id, m.seq, m.body, m.status, m.created_at, m.edited_at, m.parent_id,
       u.id AS author_id, u.username AS author_username, u.status AS author_status,
       p.seq AS parent_seq, p.status AS parent_status, left(p.body, 140) AS parent_excerpt,
       pu.username AS parent_author_username, pu.status AS parent_author_status
FROM messages m
JOIN users u        ON u.id = m.author_id
LEFT JOIN messages p ON p.id = m.parent_id
LEFT JOIN users pu   ON pu.id = p.author_id
WHERE m.discussion_id = $1 AND m.seq < $2
ORDER BY m.seq DESC
LIMIT $3;            -- limit+1 to compute has_more; reverse to ascending in Rust
```
Every join is a PK lookup, so the cost is bounded by `limit`. No N+1.

**Q3 Catch-up after a seq (WS resume / newer page)**: the same SELECT with `WHERE m.discussion_id = $1 AND m.seq > $2 ORDER BY m.seq ASC LIMIT $3`.

**Q4 Create discussion (race-safe)**, in one transaction:
```sql
INSERT INTO discussions (id, display_url, site, title, created_by) VALUES ($1, $2, $3, $4, $5);
INSERT INTO discussion_identities (key_hash, key, discussion_id, ruleset, kind)
VALUES ($6, $7, $1, 1, $8)
ON CONFLICT (key_hash) DO NOTHING
RETURNING discussion_id;
-- 0 rows returned → ROLLBACK, then run Q1 and return the existing discussion (HTTP 200 instead of 201).
```

**Q5 Insert message**, in one transaction:
```sql
-- (a) idempotency check (also re-run after a unique violation on messages_author_client_key)
SELECT id FROM messages WHERE author_id = $1 AND client_msg_id = $2;
-- (b) parent check if parent_id given: must exist in same discussion and be visible
SELECT status FROM messages WHERE discussion_id = $3 AND id = $4;
-- (c) allocate seq (row lock serializes writers per discussion only)
UPDATE discussions
   SET last_seq = last_seq + 1, message_count = message_count + 1, last_message_at = now()
 WHERE id = $3 AND status = 'open' AND merged_into_id IS NULL
RETURNING last_seq;                      -- 0 rows → 404 or 409 discussion_locked
-- (d)
INSERT INTO messages (id, discussion_id, seq, author_id, parent_id, body, client_msg_id)
VALUES ($5, $3, $6, $1, $4, $7, $2);
COMMIT;  -- then publish to the hub
```
The row lock on one discussion row caps writes per discussion at roughly 1/(transaction time), which is in the thousands per second and well above any
human chat rate. The slow-mode rate limit (Phase 6) caps viral discussions long before that.

**Q6 Authenticate a request** (unique index on `token_hash`):
```sql
SELECT s.id AS session_id, s.user_id, s.last_used_at, u.username, u.role, u.status, u.suspended_until
FROM sessions s JOIN users u ON u.id = s.user_id
WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now();
```
Sliding touch, written at most every 10 minutes per session:
```sql
UPDATE sessions SET last_used_at = now(),
       expires_at = LEAST(now() + interval '30 days', created_at + interval '180 days')
WHERE id = $1 AND last_used_at < now() - interval '10 minutes';
```

**Q7 Moderation queue** (`reports_open_queue_idx`): `SELECT … FROM reports WHERE status='open' AND created_at > $cursor ORDER BY created_at LIMIT 50`.
Cursor = `(created_at, id)`, encoded as an opaque base64 string.

**Q8 Spam/dup check**: in memory (§H.4). No query.

### D.4 Storage growth model (estimates; measure with `pg_total_relation_size`)

Assumptions: average body 200 bytes. Heap row ≈ 24 B tuple header, 3×16 B uuids, 16 B uuid client id, 8 B seq, 3×8 B timestamps,
the body and the status ≈ **340 B**. Index entries for messages (PK, (discussion_id,seq), (discussion_id,id), (author_id,client_msg_id),
(author_id,created_at), partial parent) ≈ **220 B**. Add ~20% bloat/fill factor → **≈ 650 B per message**. Discussions are about 1 per 10 messages at ≈ 600 B
including the identity row. Overall **≈ 0.7 KB per message**. A compressed `pg_dump -Fc` is about 25–35% of that.

| Messages/day | DB growth/year | Compressed dump/year of data |
|---:|---:|---:|
| 1,000 | ~0.26 GB | ~0.08 GB |
| 10,000 | ~2.6 GB | ~0.8 GB |
| 100,000 | ~26 GB | ~8 GB |
| 1,000,000 | ~255 GB | ~80 GB |

Monitoring: the `pg_database_size` metric (postgres-exporter) plus a disk-usage alert at 80%. Run the growth query in the monthly ops checklist.
Retention: sessions are purged 30 days after expiry or revocation. Removed-message bodies are kept 90 days for appeals and then erased. Resolved
reports are kept 1 year. Audit events are kept 2 years.
