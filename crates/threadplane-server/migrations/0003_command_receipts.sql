CREATE TABLE IF NOT EXISTS command_receipts (
    command_id UUID PRIMARY KEY,
    workspace TEXT NOT NULL,
    actor TEXT NOT NULL,
    command_kind TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_payload JSONB NOT NULL,
    response_payload JSONB,
    status TEXT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT command_receipts_unique_key
        UNIQUE (workspace, actor, command_kind, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_command_receipts_workspace_recorded_at
    ON command_receipts (workspace, recorded_at DESC);
