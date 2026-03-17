CREATE TABLE IF NOT EXISTS events (
    event_id UUID PRIMARY KEY,
    workspace TEXT NOT NULL,
    actor TEXT NOT NULL,
    kind TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS epics (
    epic_id UUID PRIMARY KEY,
    event_id UUID NOT NULL REFERENCES events(event_id),
    workspace TEXT NOT NULL,
    author TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    note_id UUID PRIMARY KEY,
    event_id UUID NOT NULL REFERENCES events(event_id),
    workspace TEXT NOT NULL,
    author TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    task_id UUID PRIMARY KEY,
    event_id UUID NOT NULL REFERENCES events(event_id),
    workspace TEXT NOT NULL,
    author TEXT NOT NULL,
    title TEXT NOT NULL,
    details TEXT NOT NULL,
    status TEXT NOT NULL,
    epic_id UUID NULL,
    priority TEXT NOT NULL DEFAULT 'medium',
    owner TEXT NULL,
    labels TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS task_claims (
    claim_id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(task_id),
    workspace TEXT NOT NULL,
    actor TEXT NOT NULL,
    event_id UUID NOT NULL REFERENCES events(event_id),
    claimed_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    released_at TIMESTAMPTZ NULL
);

CREATE TABLE IF NOT EXISTS links (
    link_id UUID PRIMARY KEY,
    event_id UUID NOT NULL REFERENCES events(event_id),
    workspace TEXT NOT NULL,
    actor TEXT NOT NULL,
    from_entity_ref TEXT NOT NULL,
    to_entity_ref TEXT NOT NULL,
    relation TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS task_dependencies (
    task_id UUID NOT NULL REFERENCES tasks(task_id),
    depends_on_task_id UUID NOT NULL REFERENCES tasks(task_id),
    workspace TEXT NOT NULL,
    actor TEXT NOT NULL,
    event_id UUID NOT NULL REFERENCES events(event_id),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (task_id, depends_on_task_id)
);

CREATE TABLE IF NOT EXISTS transclusion_groups (
    transclusion_id UUID PRIMARY KEY,
    workspace TEXT NOT NULL,
    created_by TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

ALTER TABLE notes ADD COLUMN IF NOT EXISTS transclusion_id UUID;
ALTER TABLE notes ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ;
ALTER TABLE epics ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ;
UPDATE epics SET updated_at = created_at WHERE updated_at IS NULL;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS transclusion_id UUID;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS epic_id UUID;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS priority TEXT NOT NULL DEFAULT 'medium';
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS owner TEXT;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS labels TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE links ADD COLUMN IF NOT EXISTS is_xanadu BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE links ADD COLUMN IF NOT EXISTS transclusion_id UUID;
UPDATE notes SET updated_at = created_at WHERE updated_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_events_workspace_created_at
ON events (workspace, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_tasks_workspace_status_created_at
ON tasks (workspace, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_tasks_workspace_epic_id_created_at
ON tasks (workspace, epic_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_tasks_workspace_priority_created_at
ON tasks (workspace, priority, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_tasks_workspace_owner_created_at
ON tasks (workspace, owner, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_tasks_labels_gin
ON tasks USING GIN (labels);

CREATE INDEX IF NOT EXISTS idx_task_claims_task_id_expires_at
ON task_claims (task_id, expires_at DESC);

CREATE INDEX IF NOT EXISTS idx_task_dependencies_task_id
ON task_dependencies (task_id);

CREATE INDEX IF NOT EXISTS idx_task_dependencies_depends_on_task_id
ON task_dependencies (depends_on_task_id);

CREATE INDEX IF NOT EXISTS idx_notes_transclusion_id
ON notes (transclusion_id);

CREATE INDEX IF NOT EXISTS idx_tasks_transclusion_id
ON tasks (transclusion_id);

CREATE INDEX IF NOT EXISTS idx_epics_workspace_created_at
ON epics (workspace, created_at DESC);
