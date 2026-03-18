use core::str::FromStr as _;

use super::*;

impl From<EpicRow> for EpicRecord {
    #[inline]
    fn from(value: EpicRow) -> Self {
        Self {
            epic_id: value.epic_id,
            entity_ref: epic_entity_ref(value.epic_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl TryFrom<MemoryRow> for MemoryRecord {
    type Error = ThreadplaneServerError;

    #[inline]
    fn try_from(value: MemoryRow) -> ServerResult<Self> {
        Ok(Self {
            memory_id: value.memory_id,
            entity_ref: memory_entity_ref(value.memory_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            kind: parse_memory_kind(&value.kind),
            scope: parse_memory_scope(&value.scope)?,
            audience: parse_memory_audience(&value.audience)?,
            importance: parse_memory_importance(&value.importance)?,
            tags: normalize_memory_tags(value.tags),
            recall_triggers: normalize_memory_recall_triggers(value.recall_triggers),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        })
    }
}

impl From<NoteRow> for NoteRecord {
    #[inline]
    fn from(value: NoteRow) -> Self {
        Self {
            note_id: value.note_id,
            entity_ref: note_entity_ref(value.note_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskRecord {
    #[inline]
    fn from(value: TaskRow) -> Self {
        let metadata = task_metadata_from_row(&value);
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            details: value.details,
            status: value.status,
            epic_id: value.epic_id,
            metadata,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskSummary {
    #[inline]
    fn from(value: TaskRow) -> Self {
        let metadata = task_metadata_from_row(&value);
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            workspace: value.workspace,
            title: value.title,
            details: value.details,
            status: value.status,
            epic_id: value.epic_id,
            author: value.author,
            metadata,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<ClaimRow> for TaskClaimRecord {
    #[inline]
    fn from(value: ClaimRow) -> Self {
        Self {
            claim_id: value.claim_id,
            task_id: value.task_id,
            workspace: value.workspace,
            actor: value.actor,
            event_id: value.event_id,
            claimed_at: value.claimed_at.to_rfc3339(),
            expires_at: value.expires_at.to_rfc3339(),
        }
    }
}

impl From<LinkRow> for LinkRecord {
    #[inline]
    fn from(value: LinkRow) -> Self {
        Self {
            link_id: value.link_id,
            event_id: value.event_id,
            workspace: value.workspace,
            actor: value.actor,
            from: value.from_entity_ref,
            to: value.to_entity_ref,
            relation: value.relation,
            is_xanadu: value.is_xanadu,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

fn task_metadata_from_row(value: &TaskRow) -> TaskMetadata {
    TaskMetadata {
        labels: normalize_task_labels(value.labels.clone()),
        owner: normalize_task_owner(value.owner.clone()),
        priority: parse_task_priority(&value.priority),
    }
}

fn parse_task_priority(value: &str) -> TaskPriority {
    TaskPriority::from_lossy(value)
}

fn parse_memory_kind(value: &str) -> MemoryKind {
    MemoryKind::from_lossy(value)
}

fn parse_memory_scope(value: &str) -> ServerResult<MemoryScope> {
    MemoryScope::from_str(value)
        .map_err(|error| ThreadplaneServerError::internal(error.to_string()))
}

fn parse_memory_audience(value: &str) -> ServerResult<MemoryAudience> {
    MemoryAudience::from_str(value)
        .map_err(|error| ThreadplaneServerError::internal(error.to_string()))
}

fn parse_memory_importance(value: &str) -> ServerResult<MemoryImportance> {
    MemoryImportance::from_str(value)
        .map_err(|error| ThreadplaneServerError::internal(error.to_string()))
}
