#![expect(
    clippy::redundant_pub_crate,
    reason = "Path builders stay crate-local and grouped by HTTP shape."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "Path builders consume the command module boundary as a concise prelude."
)]

use super::*;

pub(crate) fn entity_relations_path(entity_ref: &str) -> String {
    format!("/v1/entities/{entity_ref}/relations")
}

pub(crate) fn entity_show_path(entity_ref: &str) -> String {
    format!("/v1/entities/{entity_ref}")
}

pub(crate) fn events_list_path(workspace: &str, limit: i64) -> String {
    format!("/v1/workspaces/{workspace}/events?limit={limit}")
}

pub(crate) fn events_tail_path(
    workspace: &str,
    limit: i64,
    after_event_id: Option<Uuid>,
) -> String {
    let mut params = vec![format!("limit={limit}")];
    if let Some(event_id) = after_event_id {
        params.push(format!("after_event_id={event_id}"));
    }

    format!(
        "/v1/workspaces/{workspace}/events/tail?{}",
        params.join("&")
    )
}

pub(crate) fn memory_list_path(input: MemoryListPathArgs<'_>) -> Result<String> {
    let mut params = Vec::new();
    if let Some(query_limit) = input.limit {
        params.push(format!("limit={query_limit}"));
    }
    if let Some(selected_audience) = input.audience {
        params.push(format!(
            "audience={}",
            parse_memory_audience_input(selected_audience)?
        ));
    }
    if let Some(selected_importance) = input.importance {
        params.push(format!(
            "importance={}",
            parse_memory_importance_input(selected_importance)?
        ));
    }
    if let Some(selected_kind) = input.kind {
        params.push(format!("kind={}", parse_memory_kind_input(selected_kind)?));
    }
    if let Some(search_query) = input.query.map(str::trim).filter(|value| !value.is_empty()) {
        params.push(format!("query={search_query}"));
    }
    if let Some(selected_trigger) = input
        .recall_trigger
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        params.push(format!(
            "recall_trigger={}",
            normalize_memory_filter_name(selected_trigger)?
        ));
    }
    if let Some(selected_tag) = input.tag.map(str::trim).filter(|value| !value.is_empty()) {
        params.push(format!(
            "tag={}",
            normalize_memory_filter_name(selected_tag)?
        ));
    }

    let suffix = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };

    Ok(format!(
        "/v1/workspaces/{}/memories{suffix}",
        input.workspace
    ))
}

pub(crate) fn note_list_path(
    workspace: &str,
    limit: Option<i64>,
    author: Option<&str>,
    query: Option<&str>,
) -> String {
    let mut params = Vec::new();
    if let Some(query_limit) = limit {
        params.push(format!("limit={query_limit}"));
    }
    if let Some(selected_author) = normalize_task_owner(author.map(str::to_owned)) {
        params.push(format!("author={selected_author}"));
    }
    if let Some(search_query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        params.push(format!("query={search_query}"));
    }

    let suffix = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };

    format!("/v1/workspaces/{workspace}/notes{suffix}")
}

pub(super) fn memory_path(memory_id: Uuid) -> String {
    format!("/v1/memories/{memory_id}")
}

pub(super) fn memory_prime_path(memory: &PrimeMemories) -> Result<String> {
    let mut params = vec![
        format!(
            "audience={}",
            parse_memory_audience_input(&memory.audience)?
        ),
        format!(
            "recall_trigger={}",
            normalize_memory_filter_name(&memory.recall_trigger)?
        ),
        format!("tag={}", normalize_memory_filter_name(&memory.tag)?),
    ];
    if let Some(query_limit) = memory.limit {
        params.push(format!("limit={query_limit}"));
    }

    Ok(format!(
        "/v1/workspaces/{}/memories/prime?{}",
        memory.workspace,
        params.join("&")
    ))
}

pub(super) fn note_path(note_id: Uuid) -> String {
    format!("/v1/notes/{note_id}")
}

pub(super) fn task_claim_release_path(task_id: Uuid) -> String {
    format!("{}/claims/release", task_path(task_id))
}

pub(super) fn task_claims_path(task_id: Uuid) -> String {
    format!("{}/claims", task_path(task_id))
}

pub(super) fn task_completion_path(task_id: Uuid) -> String {
    format!("{}/completion", task_path(task_id))
}

pub(super) fn task_context_path(task_id: Uuid) -> String {
    format!("{}/context", task_path(task_id))
}

pub(super) fn task_dag_path(task_id: Uuid) -> String {
    format!("{}/dag", task_path(task_id))
}

pub(super) fn task_dependencies_path(task_id: Uuid) -> String {
    format!("{}/dependencies", task_path(task_id))
}

pub(super) fn task_list_path(task: &ListTasks) -> Result<String> {
    let suffix = task_query_suffix(
        task.status.map(TaskStatusValue::as_str),
        task.epic_id,
        task.limit,
        task.label.as_deref(),
        task.metadata_filters.owner.as_deref(),
        task.metadata_filters.priority.as_deref(),
        task.ready_only,
    )?;

    Ok(format!("/v1/workspaces/{}/tasks{}", task.workspace, suffix))
}

pub(super) fn task_next_path(task: &NextTask) -> Result<String> {
    let suffix = task_query_suffix(
        Some("open"),
        task.epic_id,
        None,
        task.label.as_deref(),
        task.metadata_filters.owner.as_deref(),
        task.metadata_filters.priority.as_deref(),
        true,
    )?;

    Ok(format!(
        "/v1/workspaces/{}/tasks/next{}",
        task.workspace, suffix
    ))
}

pub(super) fn task_path(task_id: Uuid) -> String {
    format!("/v1/tasks/{task_id}")
}

pub(super) fn workspace_keys_path(workspace: &str, actor_id: Option<&str>) -> String {
    if let Some(selected_actor_id) = normalize_task_owner(actor_id.map(str::to_owned)) {
        return format!("/v1/workspaces/{workspace}/keys?actor_id={selected_actor_id}");
    }

    format!("/v1/workspaces/{workspace}/keys")
}

pub(super) fn workspace_memberships_path(workspace: &str) -> String {
    format!("/v1/workspaces/{workspace}/memberships")
}

pub(super) fn workspace_policy_path(workspace: &str) -> String {
    format!("/v1/workspaces/{workspace}/policy")
}

fn task_query_suffix(
    status: Option<&str>,
    epic_id: Option<Uuid>,
    limit: Option<i64>,
    label: Option<&str>,
    owner: Option<&str>,
    priority: Option<&str>,
    ready_only: bool,
) -> Result<String> {
    let mut query = Vec::new();
    if let Some(status_filter) = status {
        query.push(format!("status={status_filter}"));
    }
    if let Some(selected_epic_id) = epic_id {
        query.push(format!("epic_id={selected_epic_id}"));
    }
    if let Some(query_limit) = limit {
        query.push(format!("limit={query_limit}"));
    }
    if let Some(selected_label) =
        normalize_task_labels(label.map(str::to_owned).into_iter().collect())
            .into_iter()
            .next()
    {
        query.push(format!("label={selected_label}"));
    }
    if let Some(selected_owner) = normalize_task_owner(owner.map(str::to_owned)) {
        query.push(format!("owner={selected_owner}"));
    }
    if let Some(selected_priority) = priority {
        query.push(format!(
            "priority={}",
            parse_task_priority_input(selected_priority)?
        ));
    }
    if ready_only {
        query.push("ready_only=true".to_owned());
    }

    if query.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("?{}", query.join("&")))
    }
}
