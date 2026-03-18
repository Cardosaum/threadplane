#![expect(
    clippy::redundant_pub_crate,
    reason = "Read handlers are grouped by query surface rather than alphabetically."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "Read handlers use the parent handler module as their import boundary."
)]

use super::*;

pub(crate) async fn root() -> Json<ServiceSnapshot> {
    Json(service_snapshot(current_build_info()))
}

pub(crate) async fn healthz(State(pool): State<PgPool>) -> ServerResult<Json<Value>> {
    let projection = fetch_projection_status(&pool, GRAPH_PROJECTION_NAME).await?;
    Ok(Json(with_projection_status(
        health_summary(&current_build_info()),
        projection,
    )?))
}

pub(crate) async fn scope(State(pool): State<PgPool>) -> ServerResult<Json<Value>> {
    let projection = fetch_projection_status(&pool, GRAPH_PROJECTION_NAME).await?;
    Ok(Json(with_projection_status(
        scope_summary(&current_build_info()),
        projection,
    )?))
}

pub(crate) async fn projection_status(State(pool): State<PgPool>) -> AppResult<ProjectionStatus> {
    let data = fetch_projection_status(&pool, GRAPH_PROJECTION_NAME).await?;
    Ok(success(data))
}

pub(crate) async fn show_epic(
    State(pool): State<PgPool>,
    Path(EpicPath { epic_id }): Path<EpicPath>,
) -> AppResult<EpicRecord> {
    let row = fetch_epic_by_id(&pool, epic_id).await?;
    Ok(success(EpicRecord::from(row)))
}

pub(crate) async fn show_entity(
    State(graph): State<Arc<Graph>>,
    State(pool): State<PgPool>,
    Path(EntityPath { entity_ref }): Path<EntityPath>,
) -> AppResult<EntityContext> {
    let entity = fetch_entity_record(&pool, &entity_ref).await?;
    let relations = fetch_entity_relations(graph.as_ref(), &entity_ref)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(success(EntityContext { entity, relations }))
}

pub(crate) async fn related_entities(
    State(graph): State<Arc<Graph>>,
    State(pool): State<PgPool>,
    Path(EntityPath { entity_ref }): Path<EntityPath>,
) -> AppResult<Vec<threadplane_core::GraphRelation>> {
    let _entity = fetch_entity_record(&pool, &entity_ref).await?;
    let relations = fetch_entity_relations(graph.as_ref(), &entity_ref)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(success(relations))
}

pub(crate) async fn show_memory(
    State(pool): State<PgPool>,
    Path(MemoryPath { memory_id }): Path<MemoryPath>,
) -> AppResult<MemoryRecord> {
    let row = fetch_memory_by_id(&pool, memory_id).await?;
    Ok(success(MemoryRecord::try_from(row)?))
}

pub(crate) async fn list_memories(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<MemoryListQuery>,
) -> AppResult<Vec<MemoryRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let limit = normalized_list_limit(query.limit);
    let rows = fetch_memories_for_listing(
        &pool,
        &workspace,
        MemoryListFilters {
            audience: query.audience,
            importance: query.importance,
            kind: query.kind.as_ref(),
            query: query.query.as_deref(),
            recall_trigger: query.recall_trigger.as_deref(),
            tag: query.tag.as_deref(),
        },
        limit,
    )
    .await?;
    let data = rows
        .into_iter()
        .map(MemoryRecord::try_from)
        .collect::<ServerResult<Vec<_>>>()?;
    Ok(success(data))
}

pub(crate) async fn prime_memories(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<MemoryListQuery>,
) -> AppResult<Vec<MemoryRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let limit = normalized_list_limit(query.limit);
    let rows = fetch_memories_for_listing(
        &pool,
        &workspace,
        MemoryListFilters {
            audience: Some(query.audience.unwrap_or(MemoryAudience::Agent)),
            importance: query.importance,
            kind: query.kind.as_ref(),
            query: query.query.as_deref(),
            recall_trigger: query.recall_trigger.as_deref().or(Some("session_start")),
            tag: query.tag.as_deref().or(Some("prime")),
        },
        limit,
    )
    .await?;
    let data = rows
        .into_iter()
        .map(MemoryRecord::try_from)
        .collect::<ServerResult<Vec<_>>>()?;
    Ok(success(data))
}

pub(crate) async fn show_note(
    State(pool): State<PgPool>,
    Path(NotePath { note_id }): Path<NotePath>,
) -> AppResult<NoteRecord> {
    let row = fetch_note_by_id(&pool, note_id).await?;
    Ok(success(NoteRecord::from(row)))
}

pub(crate) async fn list_notes(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<NoteListQuery>,
) -> AppResult<Vec<NoteRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let limit = normalized_list_limit(query.limit);
    let rows = fetch_notes_for_listing(
        &pool,
        &workspace,
        NoteListFilters {
            author: query.author.as_deref(),
            query: query.query.as_deref(),
        },
        limit,
    )
    .await?;
    let data = rows.into_iter().map(NoteRecord::from).collect();
    Ok(success(data))
}

pub(crate) async fn list_events(
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<ListQuery>,
) -> AppResult<Vec<EventRecord>> {
    let limit = normalized_list_limit(query.limit);
    let rows = fetch_event_rows_for_workspace(&pool, &workspace, limit).await?;
    let data = rows.into_iter().map(EventRecord::from).collect();
    Ok(success(data))
}

pub(crate) async fn tail_events(
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<EventTailQuery>,
) -> AppResult<Vec<EventRecord>> {
    let limit = normalized_list_limit(query.limit);
    let cursor = if let Some(event_id) = query.after_event_id {
        let event = fetch_event_row_for_workspace(&pool, &workspace, event_id).await?;
        Some(event.cursor())
    } else {
        None
    };
    let rows = fetch_event_rows_after_workspace_cursor(&pool, &workspace, cursor, limit).await?;
    let data = rows.into_iter().map(EventRecord::from).collect();
    Ok(success(data))
}

pub(crate) async fn list_epics(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<Vec<EpicRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let rows = fetch_epic_rows_for_workspace(&pool, &workspace).await?;
    let data = rows.into_iter().map(EpicRecord::from).collect();
    Ok(success(data))
}

pub(crate) async fn show_workspace_policy(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<WorkspacePolicy> {
    let data = ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    Ok(success(data))
}

pub(crate) async fn list_workspace_memberships(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<Vec<WorkspaceMembership>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let data = fetch_workspace_memberships(&pool, &workspace).await?;
    Ok(success(data))
}

pub(crate) async fn list_workspace_public_keys(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<WorkspaceKeysQuery>,
) -> AppResult<Vec<ActorPublicKey>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let data = fetch_actor_public_keys(&pool, &workspace, query.actor_id.as_deref()).await?;
    Ok(success(data))
}

pub(crate) async fn list_tasks(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<TaskListQuery>,
) -> AppResult<Vec<TaskListEntry>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let limit = normalized_list_limit(query.limit);
    let rows = fetch_tasks_for_listing(
        &pool,
        &workspace,
        task_selection_filters(&query),
        Some(limit),
    )
    .await?;
    let data = build_task_list_entries(&pool, rows).await?;
    Ok(success(data))
}

pub(crate) async fn next_task(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<TaskListQuery>,
) -> AppResult<Option<TaskListEntry>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let row = fetch_tasks_for_listing(&pool, &workspace, task_next_filters(&query), Some(1))
        .await?
        .into_iter()
        .next();
    let data = if let Some(task) = row {
        let mut entries = build_task_list_entries(&pool, vec![task]).await?;
        entries.pop()
    } else {
        None
    };

    Ok(success(data))
}

pub(crate) async fn list_open_tasks(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<Vec<TaskListEntry>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let rows = fetch_tasks_for_listing(
        &pool,
        &workspace,
        TaskListFilters {
            ready_only: false,
            status: Some("open"),
            ..TaskListFilters::default()
        },
        None,
    )
    .await?;
    let data = build_task_list_entries(&pool, rows).await?;
    Ok(success(data))
}

pub(crate) async fn show_task(
    State(pool): State<PgPool>,
    Path(TaskPath { task_id }): Path<TaskPath>,
) -> AppResult<TaskRecord> {
    let data = TaskRecord::from(fetch_task_by_id(&pool, task_id).await?);
    Ok(success(data))
}

pub(crate) async fn task_context(
    State(graph): State<Arc<Graph>>,
    State(pool): State<PgPool>,
    Path(TaskPath { task_id }): Path<TaskPath>,
) -> AppResult<TaskContext> {
    let task = fetch_task_by_id(&pool, task_id).await?;
    let active_claim = fetch_active_claim(&pool, task_id)
        .await?
        .map(TaskClaimRecord::from);
    let epic = fetch_epic_for_task(&pool, &task).await?;
    let dependencies = fetch_direct_dependencies(&pool, task_id).await?;
    let dependents = fetch_direct_dependents(&pool, task_id).await?;
    let relations =
        fetch_entity_relations(graph.as_ref(), &threadplane_core::task_entity_ref(task_id))
            .await
            .map_err(ThreadplaneServerError::internal)?;

    let data = TaskContext {
        task: task.clone().into(),
        active_claim,
        dependencies,
        dependents,
        epic,
        ready: task_is_ready(&pool, task_id).await?,
        relations,
    };

    Ok(success(data))
}

pub(crate) async fn task_dag(
    State(pool): State<PgPool>,
    Path(TaskPath { task_id }): Path<TaskPath>,
) -> AppResult<TaskDag> {
    let task = fetch_task_by_id(&pool, task_id).await?;
    let data = TaskDag {
        task: task.clone().into(),
        epic: fetch_epic_for_task(&pool, &task).await?,
        ready: task_is_ready(&pool, task_id).await?,
        dependencies: fetch_dependency_chain(&pool, task_id).await?,
        dependents: fetch_dependent_chain(&pool, task_id).await?,
    };

    Ok(success(data))
}
