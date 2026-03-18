use super::*;

#[derive(Debug, FromRow, Clone)]
pub(crate) struct EpicRow {
    pub(crate) epic_id: Uuid,
    pub(crate) event_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) author: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct MemoryRow {
    pub(crate) memory_id: Uuid,
    pub(crate) event_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) author: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) kind: String,
    pub(crate) scope: String,
    pub(crate) audience: String,
    pub(crate) importance: String,
    pub(crate) tags: Vec<String>,
    pub(crate) recall_triggers: Vec<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct NoteRow {
    pub(crate) note_id: Uuid,
    pub(crate) event_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) author: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) transclusion_id: Option<Uuid>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct TaskRow {
    pub(crate) task_id: Uuid,
    pub(crate) event_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) author: String,
    pub(crate) title: String,
    pub(crate) details: String,
    pub(crate) status: String,
    pub(crate) epic_id: Option<Uuid>,
    pub(crate) priority: String,
    pub(crate) owner: Option<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) transclusion_id: Option<Uuid>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct ClaimRow {
    pub(crate) claim_id: Uuid,
    pub(crate) task_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) actor: String,
    pub(crate) event_id: Uuid,
    pub(crate) claimed_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    #[sqlx(rename = "released_at")]
    pub(crate) _released_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
pub(crate) struct LinkRow {
    pub(crate) link_id: Uuid,
    pub(crate) event_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) actor: String,
    pub(crate) from_entity_ref: String,
    pub(crate) to_entity_ref: String,
    pub(crate) relation: String,
    pub(crate) is_xanadu: bool,
    pub(crate) transclusion_id: Option<Uuid>,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct TaskDependencyRow {
    pub(crate) task_id: Uuid,
    pub(crate) depends_on_task_id: Uuid,
}

#[derive(Debug, FromRow)]
pub(crate) struct TaskDependencyListRow {
    pub(crate) dependency_id: Uuid,
    pub(crate) source_task_id: Uuid,
    pub(crate) status: String,
    pub(crate) title: String,
}

#[derive(Debug, FromRow)]
pub(crate) struct TaskReadyRow {
    pub(crate) ready: bool,
    pub(crate) task_id: Uuid,
}

#[derive(Debug, FromRow)]
pub(crate) struct TransclusionGroupRow {
    pub(crate) title: String,
    pub(crate) content: String,
    pub(crate) updated_at: DateTime<Utc>,
}
