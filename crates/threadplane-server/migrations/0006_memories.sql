CREATE TABLE IF NOT EXISTS memories (
    memory_id UUID PRIMARY KEY,
    event_id UUID NOT NULL REFERENCES events(event_id) ON DELETE CASCADE,
    workspace TEXT NOT NULL,
    author TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    kind TEXT NOT NULL,
    scope TEXT NOT NULL,
    audience TEXT NOT NULL,
    importance TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    recall_triggers TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_workspace_updated_at
    ON memories (workspace, updated_at DESC, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_memories_event_id
    ON memories (event_id);

CREATE INDEX IF NOT EXISTS idx_memories_workspace_kind
    ON memories (workspace, kind);

CREATE INDEX IF NOT EXISTS idx_memories_workspace_importance
    ON memories (workspace, importance);

CREATE INDEX IF NOT EXISTS idx_memories_workspace_audience
    ON memories (workspace, audience);

CREATE INDEX IF NOT EXISTS idx_memories_workspace_scope
    ON memories (workspace, scope);

CREATE INDEX IF NOT EXISTS idx_memories_tags_gin
    ON memories USING GIN (tags);

CREATE INDEX IF NOT EXISTS idx_memories_recall_triggers_gin
    ON memories USING GIN (recall_triggers);
