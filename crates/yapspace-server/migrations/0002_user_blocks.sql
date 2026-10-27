CREATE TABLE user_blocks (
    blocker_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    blocked_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CONSTRAINT user_blocks_not_self CHECK (blocker_id <> blocked_id)
);
