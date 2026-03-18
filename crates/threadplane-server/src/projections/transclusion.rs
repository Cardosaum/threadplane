use neo4rs::query;
use sqlx::query_as;

use crate::{
    prelude::*,
    storage::{NoteRow, TaskRow, NOTE_SELECT, TASK_SELECT},
};
use threadplane_core::{NoteRecord, TaskRecord};

use super::{project_note, project_task};

pub(crate) async fn reproject_transclusion_group(
    graph: &Graph,
    pool: &PgPool,
    group_id: Uuid,
    merged_group_id: Option<Uuid>,
) -> ServerResult<()> {
    if let Some(old_group_id) = merged_group_id {
        graph
            .run(
                query("MATCH ()-[rel:XANADU_LINK {transclusion_id: $group_id}]-() DELETE rel")
                    .param("group_id", old_group_id.to_string()),
            )
            .await?;
    }

    graph
        .run(
            query("MATCH ()-[rel:XANADU_LINK {transclusion_id: $group_id}]-() DELETE rel")
                .param("group_id", group_id.to_string()),
        )
        .await?;

    let notes: Vec<NoteRow> = query_as(&format!(
        "
        {NOTE_SELECT}
        WHERE transclusion_id = $1
        ORDER BY note_id
        "
    ))
    .bind(group_id)
    .fetch_all(pool)
    .await?;

    let tasks: Vec<TaskRow> = query_as(&format!(
        "
        {TASK_SELECT}
        WHERE transclusion_id = $1
        ORDER BY task_id
        "
    ))
    .bind(group_id)
    .fetch_all(pool)
    .await?;

    let mut entity_refs = Vec::new();

    for note in notes {
        let record = NoteRecord::from(note);
        entity_refs.push(record.entity_ref.clone());
        project_note(graph, &record).await?;
    }

    for task in tasks {
        let record = TaskRecord::from(task);
        entity_refs.push(record.entity_ref.clone());
        project_task(graph, &record).await?;
    }

    entity_refs.sort();
    for (index, left) in entity_refs.iter().enumerate() {
        for right in entity_refs
            .iter()
            .skip(index.checked_add(1).unwrap_or(entity_refs.len()))
        {
            graph
                .run(
                    query(
                        "
                        MATCH (from:Entity {entity_ref: $from}), (to:Entity {entity_ref: $to})
                        MERGE (from)-[rel:XANADU_LINK]->(to)
                        SET rel.transclusion_id = $group_id
                        ",
                    )
                    .param("from", left.clone())
                    .param("to", right.clone())
                    .param("group_id", group_id.to_string()),
                )
                .await?;
        }
    }

    Ok(())
}
