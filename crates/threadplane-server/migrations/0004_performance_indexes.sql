CREATE INDEX IF NOT EXISTS idx_events_created_at_event_id
    ON events (created_at ASC, event_id ASC);

CREATE INDEX IF NOT EXISTS idx_epics_event_id
    ON epics (event_id);

CREATE INDEX IF NOT EXISTS idx_notes_event_id
    ON notes (event_id);

CREATE INDEX IF NOT EXISTS idx_tasks_event_id
    ON tasks (event_id);

CREATE INDEX IF NOT EXISTS idx_task_claims_event_id
    ON task_claims (event_id);

CREATE INDEX IF NOT EXISTS idx_links_event_id
    ON links (event_id);

CREATE INDEX IF NOT EXISTS idx_task_dependencies_event_id
    ON task_dependencies (event_id);

CREATE INDEX IF NOT EXISTS idx_task_claims_active_task_claimed_at
    ON task_claims (task_id, claimed_at DESC)
    WHERE released_at IS NULL;
