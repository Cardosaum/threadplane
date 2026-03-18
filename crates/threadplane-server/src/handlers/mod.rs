#![expect(
    clippy::redundant_pub_crate,
    reason = "Handlers are crate-local endpoints with explicit visibility."
)]

mod content;
mod links;
mod params;
mod reads;
mod shared;
mod tasks;
mod workspace;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use bon::builder;
use tracing::error;

use crate::{
    build_info::current_build_info,
    idempotency::{
        begin_idempotent_command, complete_idempotent_command, CommandExecution, IdempotencyContext,
    },
    lifecycle::{calculate_claim_expiry, normalized_lease_seconds},
    prelude::*,
    projections::{
        fetch_entity_relations, project_claim, project_epic, project_link, project_memory,
        project_note, project_task, project_task_dependency_by_id,
        project_task_supporting_entities, reproject_transclusion_group,
    },
    replay::GRAPH_PROJECTION_NAME,
    storage::{
        append_event, append_task_dependency, build_task_list_entries, fetch_active_claim,
        fetch_active_claim_tx, fetch_actor_public_keys, fetch_dependency_chain,
        fetch_dependent_chain, fetch_direct_dependencies, fetch_direct_dependents,
        fetch_entity_record, fetch_epic_by_id, fetch_epic_by_id_tx, fetch_epic_for_task,
        fetch_epic_rows_for_workspace, fetch_event_row_for_workspace,
        fetch_event_rows_after_workspace_cursor, fetch_event_rows_for_workspace,
        fetch_memories_for_listing, fetch_memory_by_id, fetch_memory_by_id_tx, fetch_note_by_id,
        fetch_note_by_id_tx, fetch_notes_for_listing, fetch_projection_status, fetch_task_by_id,
        fetch_task_by_id_tx, fetch_tasks_for_listing, fetch_workspace_memberships,
        prepare_xanadu_group, sync_transclusion_members, task_is_ready, unique_task_ids,
        update_transclusion_group, upsert_actor_public_key, upsert_workspace_membership,
        upsert_workspace_policy, workspace_supports_priority, MemoryListFilters, NoteListFilters,
        TaskListFilters, TaskRow,
    },
};

use threadplane_core::{
    health_summary, normalize_memory_recall_triggers, normalize_memory_tags, normalize_task_labels,
    normalize_task_owner, scope_summary, service_snapshot, ActorPublicKey, AddLinkRequest,
    AddTaskDependencyRequest, AddWorkspacePublicKeyRequest, ClaimNextTaskRequest, ClaimTaskRequest,
    CompleteTaskRequest, CreateEpicRequest, CreateMemoryRequest, CreateNoteRequest,
    CreateXanaduLinkRequest, EntityContext, EpicRecord, EventKind, EventRecord,
    GrantWorkspaceMembershipRequest, LinkRecord, MemoryAudience, MemoryRecord, NoteRecord,
    OfferTaskRequest, ProjectionStatus, ReleaseTaskRequest, ServiceSnapshot, TaskClaimRecord,
    TaskContext, TaskDag, TaskListEntry, TaskPriority, TaskRecord, UpdateNoteRequest,
    UpdateTaskRequest, UpdateWorkspacePolicyRequest, WorkspaceMembership, WorkspacePolicy,
    DEPENDS_ON_RELATION, XANADU_RELATION,
};

pub(crate) use content::{create_epic, create_memory, create_note, update_note};
pub(crate) use links::{add_link, add_xanadu_link};
pub(crate) use params::*;
pub(crate) use reads::{
    healthz, list_epics, list_events, list_memories, list_notes, list_open_tasks, list_tasks,
    list_workspace_memberships, list_workspace_public_keys, next_task, prime_memories,
    projection_status, related_entities, root, scope, show_entity, show_epic, show_memory,
    show_note, show_task, show_workspace_policy, tail_events, task_context, task_dag,
};
pub(crate) use shared::*;
pub(crate) use tasks::{
    add_task_dependency, claim_next_task, claim_task, complete_task, offer_task, release_task,
    task_next_filters, task_selection_filters, update_task,
};
pub(crate) use workspace::{
    add_workspace_public_key, grant_workspace_membership, update_workspace_policy,
};
