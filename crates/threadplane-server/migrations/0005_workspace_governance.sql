CREATE TABLE IF NOT EXISTS workspace_policies (
    workspace TEXT PRIMARY KEY,
    default_priority TEXT NOT NULL,
    allowed_algorithms TEXT[] NOT NULL,
    challenge_ttl_seconds INTEGER NOT NULL,
    signed_commands_required BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS workspace_priorities (
    workspace TEXT NOT NULL REFERENCES workspace_policies(workspace) ON DELETE CASCADE,
    name TEXT NOT NULL,
    rank INTEGER NOT NULL,
    description TEXT,
    PRIMARY KEY (workspace, name),
    UNIQUE (workspace, rank)
);

CREATE TABLE IF NOT EXISTS workspace_memberships (
    workspace TEXT NOT NULL REFERENCES workspace_policies(workspace) ON DELETE CASCADE,
    actor_id TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace, actor_id)
);

CREATE TABLE IF NOT EXISTS actor_public_keys (
    workspace TEXT NOT NULL REFERENCES workspace_policies(workspace) ON DELETE CASCADE,
    actor_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace, actor_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_workspace_priorities_workspace_rank
    ON workspace_priorities (workspace, rank DESC);

CREATE INDEX IF NOT EXISTS idx_workspace_memberships_workspace_role
    ON workspace_memberships (workspace, role);

CREATE INDEX IF NOT EXISTS idx_actor_public_keys_workspace_actor
    ON actor_public_keys (workspace, actor_id);
