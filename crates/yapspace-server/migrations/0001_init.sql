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
