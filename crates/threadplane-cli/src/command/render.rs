#![expect(
    clippy::redundant_pub_crate,
    reason = "Render helpers stay crate-local and grouped by output concern."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "Render helpers treat the command module boundary as their local prelude."
)]

use super::*;

pub(crate) fn render_entity_context_compact(context: &EntityContext) -> String {
    let mut rendered = compact_entity_summary(&context.entity);
    rendered.push('\n');
    rendered.push_str(&render_graph_relations_compact(&context.relations));
    rendered
}

pub(crate) fn render_event_list_compact(entries: &[EventRecord]) -> String {
    if entries.is_empty() {
        return "no events\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | actor={} | at={}",
                short_uuid(&entry.event_id),
                entry.kind,
                entry.actor,
                entry.created_at
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_graph_relations_compact(entries: &[GraphRelation]) -> String {
    if entries.is_empty() {
        return "no related entities\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            let title = entry.title.as_deref().unwrap_or("untitled");
            format!(
                "{} {} | {} | {}",
                entry.direction,
                entry.relation,
                short_entity_ref(entry.entity_ref.as_str()),
                title
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_memory_list_compact(entries: &[MemoryRecord]) -> String {
    if entries.is_empty() {
        return "no memories\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | kind={} | importance={} | audience={} | tags={}",
                short_uuid(&entry.memory_id),
                entry.title,
                entry.kind,
                entry.importance,
                entry.audience,
                entry.tags.join(",")
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_note_list_compact(entries: &[NoteRecord]) -> String {
    if entries.is_empty() {
        return "no notes\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | author={} | updated_at={}",
                short_uuid(&entry.note_id),
                entry.title,
                entry.author,
                entry.updated_at
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_task_dependency_compact(entries: &[TaskDependencySummary]) -> String {
    if entries.is_empty() {
        return "no tasks\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | status={} | depth={}",
                short_task_id(&entry.task_id),
                entry.title,
                entry.status,
                entry.depth
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

pub(crate) fn render_task_list_compact(entries: &[TaskListEntry]) -> String {
    if entries.is_empty() {
        return "no tasks\n".to_owned();
    }

    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "{} | {} | status={} | priority={} | {} | deps={} | dependents={} | {} | {} | {} | {}",
                short_task_id(&entry.task.task_id),
                entry.task.title,
                entry.task.status,
                entry.task.metadata.priority,
                if entry.ready { "ready" } else { "blocked" },
                entry.dependencies.len(),
                entry.dependents.len(),
                compact_epic_label(entry),
                compact_owner_label(entry),
                compact_labels_label(entry),
                compact_claim_label(entry),
            )
        })
        .collect::<Vec<_>>();

    format!("{}\n", lines.join("\n"))
}

fn compact_claim_label(entry: &TaskListEntry) -> String {
    entry.active_claim.as_ref().map_or_else(
        || "claim=open".to_owned(),
        |claim| format!("claim={}", claim.actor),
    )
}

fn compact_entity_summary(entity: &EntityRecord) -> String {
    match entity {
        EntityRecord::Epic(record) => format!(
            "epic {} | {} | author={} | workspace={}",
            short_uuid(&record.epic_id),
            record.title,
            record.author,
            record.workspace
        ),
        EntityRecord::Memory(record) => format!(
            "memory {} | {} | kind={} | importance={} | workspace={}",
            short_uuid(&record.memory_id),
            record.title,
            record.kind,
            record.importance,
            record.workspace
        ),
        EntityRecord::Note(record) => format!(
            "note {} | {} | author={} | workspace={}",
            short_uuid(&record.note_id),
            record.title,
            record.author,
            record.workspace
        ),
        EntityRecord::Task(record) => format!(
            "task {} | {} | status={} | priority={} | owner={} | workspace={}",
            short_uuid(&record.task_id),
            record.title,
            record.status,
            record.metadata.priority,
            record.metadata.owner.as_deref().unwrap_or("none"),
            record.workspace
        ),
    }
}

fn compact_epic_label(entry: &TaskListEntry) -> String {
    entry.epic.as_ref().map_or_else(
        || "epic=none".to_owned(),
        |epic| format!("epic={}", epic.title),
    )
}

fn compact_labels_label(entry: &TaskListEntry) -> String {
    if entry.task.metadata.labels.is_empty() {
        return "labels=-".to_owned();
    }

    format!("labels={}", entry.task.metadata.labels.join(","))
}

fn compact_owner_label(entry: &TaskListEntry) -> String {
    entry
        .task
        .metadata
        .owner
        .as_ref()
        .map_or_else(|| "owner=none".to_owned(), |owner| format!("owner={owner}"))
}

fn short_entity_ref(entity_ref: &str) -> String {
    let Some((kind, raw_id)) = entity_ref.split_once(':') else {
        return entity_ref.to_owned();
    };
    let short_id = raw_id.split('-').next().unwrap_or(raw_id);
    format!("{kind}:{short_id}")
}

fn short_task_id(task_id: &Uuid) -> String {
    short_uuid(task_id)
}

fn short_uuid(value: &Uuid) -> String {
    value
        .to_string()
        .split('-')
        .next()
        .unwrap_or_default()
        .to_owned()
}
