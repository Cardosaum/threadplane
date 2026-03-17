CREATE TABLE IF NOT EXISTS projection_offsets (
    projection_name TEXT PRIMARY KEY,
    last_event_created_at TIMESTAMPTZ,
    last_event_id UUID,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projection_offsets_updated_at
    ON projection_offsets (updated_at DESC);
