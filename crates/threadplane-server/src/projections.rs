#![expect(
    clippy::redundant_pub_crate,
    reason = "Projection helpers are crate-local adapters around Neo4j."
)]

use neo4rs::{query, Graph};
use sqlx::query_as;
use uuid::Uuid;

use crate::{
    app::AppState,
    error::ServerResult,
    storage::{
        fetch_direct_dependencies, fetch_epic_by_id, fetch_task_by_id, NoteRow, TaskRow,
        NOTE_SELECT, TASK_SELECT,
    },
};
use threadplane_core::{
    epic_entity_ref, relation_type, task_entity_ref, EpicRecord, GraphRelation, LinkRecord,
    NoteRecord, TaskClaimRecord, TaskRecord,
};

pub(crate) async fn fetch_task_relations(
    graph: &Graph,
    task_id: Uuid,
) -> ServerResult<Vec<GraphRelation>> {
    let task_ref = task_entity_ref(task_id);
    let mut result = graph
        .execute(
            query(
                "
                MATCH (task:Entity {entity_ref: $task_ref})
                OPTIONAL MATCH (task)-[rel]-(other:Entity)
                RETURN
                  DISTINCT type(rel) AS relation,
                  CASE
                    WHEN rel IS NULL THEN NULL
                    WHEN startNode(rel).entity_ref = $task_ref THEN 'outgoing'
                    ELSE 'incoming'
                  END AS direction,
                  other.entity_ref AS entity_ref,
                  coalesce(other.kind, 'unknown') AS entity_kind,
                  other.title AS title,
                  coalesce(other.body, other.details) AS body,
                  NULLIF(other.transclusion_id, '') AS transclusion_id
                ORDER BY relation, entity_ref
                ",
            )
            .param("task_ref", task_ref),
        )
        .await?;

    let mut relations = Vec::new();
    loop {
        let maybe_row = result.next().await?;
        let Some(row) = maybe_row else {
            break;
        };

        let relation_opt: Option<String> = row.get("relation")?;
        let entity_ref_opt: Option<String> = row.get("entity_ref")?;
        let entity_kind_opt: Option<String> = row.get("entity_kind")?;
        let direction_opt: Option<String> = row.get("direction")?;
        let title: Option<String> = row.get("title")?;
        let body: Option<String> = row.get("body")?;
        let transclusion_id: Option<String> = row.get("transclusion_id")?;

        if let (Some(relation), Some(entity_ref), Some(entity_kind), Some(direction)) =
            (relation_opt, entity_ref_opt, entity_kind_opt, direction_opt)
        {
            relations.push(GraphRelation {
                relation,
                direction,
                entity_ref,
                entity_kind,
                title,
                body,
                transclusion_id: transclusion_id.and_then(|raw| Uuid::parse_str(&raw).ok()),
            });
        }
    }

    Ok(deduplicate_graph_relations(relations))
}

pub(crate) fn deduplicate_graph_relations(mut relations: Vec<GraphRelation>) -> Vec<GraphRelation> {
    relations.sort_unstable();
    relations.dedup();
    relations
}

pub(crate) async fn project_note(graph: &Graph, note: &NoteRecord) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (workspace:Workspace {name: $workspace})
                MERGE (actor:Actor {name: $actor})
                MERGE (event:Event {event_id: $event_id})
                MERGE (note:Entity:Note {entity_ref: $entity_ref})
                SET note.kind = 'note',
                    note.note_id = $note_id,
                    note.workspace = $workspace,
                    note.title = $title,
                    note.body = $body,
                    note.transclusion_id = $transclusion_id,
                    note.created_at = $created_at,
                    note.updated_at = $updated_at
                MERGE (actor)-[:AUTHORED]->(note)
                MERGE (note)-[:RECORDED_IN]->(workspace)
                MERGE (note)-[:FROM_EVENT]->(event)
                MERGE (actor)-[:EMITTED]->(event)
                ",
            )
            .param("workspace", note.workspace.clone())
            .param("actor", note.author.clone())
            .param("event_id", note.event_id.to_string())
            .param("entity_ref", note.entity_ref.clone())
            .param("note_id", note.note_id.to_string())
            .param("title", note.title.clone())
            .param("body", note.body.clone())
            .param(
                "transclusion_id",
                note.transclusion_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .param("created_at", note.created_at.clone())
            .param("updated_at", note.updated_at.clone()),
        )
        .await?;
    Ok(())
}

pub(crate) async fn project_epic(graph: &Graph, epic: &EpicRecord) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (workspace:Workspace {name: $workspace})
                MERGE (actor:Actor {name: $actor})
                MERGE (event:Event {event_id: $event_id})
                MERGE (epic:Entity:Epic {entity_ref: $entity_ref})
                SET epic.kind = 'epic',
                    epic.epic_id = $epic_id,
                    epic.workspace = $workspace,
                    epic.title = $title,
                    epic.body = $body,
                    epic.created_at = $created_at,
                    epic.updated_at = $updated_at
                MERGE (actor)-[:AUTHORED]->(epic)
                MERGE (epic)-[:RECORDED_IN]->(workspace)
                MERGE (epic)-[:FROM_EVENT]->(event)
                MERGE (actor)-[:EMITTED]->(event)
                ",
            )
            .param("workspace", epic.workspace.clone())
            .param("actor", epic.author.clone())
            .param("event_id", epic.event_id.to_string())
            .param("entity_ref", epic.entity_ref.clone())
            .param("epic_id", epic.epic_id.to_string())
            .param("title", epic.title.clone())
            .param("body", epic.body.clone())
            .param("created_at", epic.created_at.clone())
            .param("updated_at", epic.updated_at.clone()),
        )
        .await?;
    Ok(())
}

pub(crate) async fn project_task(graph: &Graph, task: &TaskRecord) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (workspace:Workspace {name: $workspace})
                MERGE (actor:Actor {name: $actor})
                MERGE (event:Event {event_id: $event_id})
                MERGE (task:Entity:Task {entity_ref: $entity_ref})
                SET task.kind = 'task',
                    task.task_id = $task_id,
                    task.workspace = $workspace,
                    task.title = $title,
                    task.details = $details,
                    task.status = $status,
                    task.epic_id = $epic_id,
                    task.priority = $priority,
                    task.owner = $owner,
                    task.labels = $labels,
                    task.transclusion_id = $transclusion_id,
                    task.created_at = $created_at,
                    task.updated_at = $updated_at
                WITH workspace, actor, event, task
                OPTIONAL MATCH (task)-[old:IMPLEMENTS_EPIC]->(:Entity:Epic)
                DELETE old
                WITH workspace, actor, event, task
                FOREACH (_ IN CASE WHEN $epic_ref = '' THEN [] ELSE [1] END |
                    MERGE (epic:Entity:Epic {entity_ref: $epic_ref})
                    ON CREATE SET epic.kind = 'epic'
                    MERGE (task)-[:IMPLEMENTS_EPIC]->(epic)
                )
                MERGE (actor)-[:AUTHORED]->(task)
                MERGE (task)-[:RECORDED_IN]->(workspace)
                MERGE (task)-[:FROM_EVENT]->(event)
                MERGE (actor)-[:EMITTED]->(event)
                ",
            )
            .param("workspace", task.workspace.clone())
            .param("actor", task.author.clone())
            .param("event_id", task.event_id.to_string())
            .param("entity_ref", task.entity_ref.clone())
            .param("task_id", task.task_id.to_string())
            .param("title", task.title.clone())
            .param("details", task.details.clone())
            .param("status", task.status.clone())
            .param(
                "epic_id",
                task.epic_id.map(|id| id.to_string()).unwrap_or_default(),
            )
            .param("priority", task.metadata.priority.to_string())
            .param("owner", task.metadata.owner.clone().unwrap_or_default())
            .param("labels", task.metadata.labels.clone())
            .param(
                "epic_ref",
                task.epic_id.map(epic_entity_ref).unwrap_or_default(),
            )
            .param(
                "transclusion_id",
                task.transclusion_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .param("created_at", task.created_at.clone())
            .param("updated_at", task.updated_at.clone()),
        )
        .await?;
    Ok(())
}

pub(crate) async fn project_task_dependency(
    graph: &Graph,
    task: &TaskRecord,
    dependency: &TaskRecord,
) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (task:Entity:Task {entity_ref: $task_ref})
                MERGE (dependency:Entity:Task {entity_ref: $dependency_ref})
                MERGE (task)-[:DEPENDS_ON]->(dependency)
                ",
            )
            .param("task_ref", task.entity_ref.clone())
            .param("dependency_ref", dependency.entity_ref.clone()),
        )
        .await?;
    Ok(())
}

pub(crate) async fn project_task_dependency_by_id(
    graph: &Graph,
    pool: &sqlx::PgPool,
    task_id: Uuid,
    depends_on_task_id: Uuid,
) -> ServerResult<()> {
    let task = TaskRecord::from(fetch_task_by_id(pool, task_id).await?);
    let dependency = TaskRecord::from(fetch_task_by_id(pool, depends_on_task_id).await?);
    project_task(graph, &task).await?;
    project_task(graph, &dependency).await?;
    project_task_dependency(graph, &task, &dependency).await
}

pub(crate) async fn project_task_supporting_entities(
    state: &AppState,
    task: &TaskRecord,
) -> ServerResult<()> {
    if let Some(epic_id) = task.epic_id {
        let epic = EpicRecord::from(fetch_epic_by_id(state.pool(), epic_id).await?);
        project_epic(state.graph(), &epic).await?;
    }

    for dependency in fetch_direct_dependencies(state.pool(), task.task_id).await? {
        project_task_dependency_by_id(
            state.graph(),
            state.pool(),
            task.task_id,
            dependency.task_id,
        )
        .await?;
    }

    Ok(())
}

pub(crate) async fn project_claim(
    graph: &Graph,
    task: &TaskRow,
    claim: &TaskClaimRecord,
) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (actor:Actor {name: $actor})
                MERGE (task:Entity:Task {entity_ref: $task_ref})
                MERGE (event:Event {event_id: $event_id})
                MERGE (claim:Claim {claim_id: $claim_id})
                SET claim.workspace = $workspace,
                    claim.claimed_at = $claimed_at,
                    claim.expires_at = $expires_at
                MERGE (claim)-[:FOR_TASK]->(task)
                MERGE (claim)-[:HELD_BY]->(actor)
                MERGE (claim)-[:FROM_EVENT]->(event)
                SET task.status = $task_status
                ",
            )
            .param("actor", claim.actor.clone())
            .param("task_ref", task_entity_ref(task.task_id))
            .param("event_id", claim.event_id.to_string())
            .param("claim_id", claim.claim_id.to_string())
            .param("workspace", claim.workspace.clone())
            .param("claimed_at", claim.claimed_at.clone())
            .param("expires_at", claim.expires_at.clone())
            .param("task_status", task.status.clone()),
        )
        .await?;
    Ok(())
}

pub(crate) async fn project_link(graph: &Graph, link: &LinkRecord) -> ServerResult<()> {
    let relation = relation_type(&link.relation);
    let cypher = format!(
        "
        MERGE (event:Event {{event_id: $event_id}})
        MERGE (from:Entity {{entity_ref: $from}})
        ON CREATE SET from.kind = 'unknown'
        MERGE (to:Entity {{entity_ref: $to}})
        ON CREATE SET to.kind = 'unknown'
        MERGE (from)-[rel:{relation}]->(to)
        SET rel.workspace = $workspace,
            rel.actor = $actor,
            rel.created_at = $created_at,
            rel.event_id = $event_id,
            rel.is_xanadu = $is_xanadu,
            rel.transclusion_id = $transclusion_id
        MERGE (from)-[:LINKED_BY_EVENT]->(event)
        MERGE (to)-[:LINKED_BY_EVENT]->(event)
        "
    );

    graph
        .run(
            query(&cypher)
                .param("event_id", link.event_id.to_string())
                .param("from", link.from.clone())
                .param("to", link.to.clone())
                .param("workspace", link.workspace.clone())
                .param("actor", link.actor.clone())
                .param("created_at", link.created_at.clone())
                .param("is_xanadu", link.is_xanadu)
                .param(
                    "transclusion_id",
                    link.transclusion_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                ),
        )
        .await?;
    Ok(())
}

pub(crate) async fn reproject_transclusion_group(
    state: &AppState,
    group_id: Uuid,
    merged_group_id: Option<Uuid>,
) -> ServerResult<()> {
    if let Some(old_group_id) = merged_group_id {
        state
            .graph()
            .run(
                query("MATCH ()-[rel:XANADU_LINK {transclusion_id: $group_id}]-() DELETE rel")
                    .param("group_id", old_group_id.to_string()),
            )
            .await?;
    }

    state
        .graph()
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
    .fetch_all(state.pool())
    .await?;

    let tasks: Vec<TaskRow> = query_as(&format!(
        "
        {TASK_SELECT}
        WHERE transclusion_id = $1
        ORDER BY task_id
        "
    ))
    .bind(group_id)
    .fetch_all(state.pool())
    .await?;

    let mut entity_refs = Vec::new();

    for note in notes {
        let record = NoteRecord::from(note);
        entity_refs.push(record.entity_ref.clone());
        project_note(state.graph(), &record).await?;
    }

    for task in tasks {
        let record = TaskRecord::from(task);
        entity_refs.push(record.entity_ref.clone());
        project_task(state.graph(), &record).await?;
    }

    entity_refs.sort();
    for (index, left) in entity_refs.iter().enumerate() {
        for right in entity_refs
            .iter()
            .skip(index.checked_add(1).unwrap_or(entity_refs.len()))
        {
            state
                .graph()
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
