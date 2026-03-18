mod build;
mod lifecycle;
mod migration;
mod projection;

use core::sync::atomic::{AtomicUsize, Ordering};

use proptest::arbitrary::any;
use proptest::prop_assert_eq;
use rstest::rstest;
use tokio::sync::Barrier;
use tokio_util::sync::CancellationToken;

use crate::{
    build_info::current_build_info,
    handlers::normalized_list_limit,
    lifecycle::{
        calculate_claim_expiry, normalized_lease_seconds, wait_for_shutdown, MINIMUM_LEASE_SECONDS,
    },
    migration::{
        COMMAND_RECEIPTS_MIGRATION_SQL, INITIAL_MIGRATION_SQL, MEMORIES_MIGRATION_SQL,
        PERFORMANCE_INDEXES_MIGRATION_SQL, PROJECTION_OFFSETS_MIGRATION_SQL,
        WORKSPACE_GOVERNANCE_MIGRATION_SQL,
    },
    prelude::*,
    projections::deduplicate_graph_relations,
    storage::{build_projection_status, event_kind_name, parse_event_kind, ProjectionCursor},
};
use threadplane_core::{EventKind, GraphRelation, TaskPriority};
