#![expect(
    clippy::redundant_pub_crate,
    reason = "Replay helpers are crate-local orchestration around durable projections."
)]

mod batch;
mod projectors;
mod worker;

use core::time::Duration;

use serde::Deserialize;

use crate::{prelude::*, storage::EventRow};

pub(crate) use batch::catch_up_graph_projection;
pub(crate) use worker::spawn_graph_projection_worker;

pub(crate) const GRAPH_PROJECTION_NAME: &str = "neo4j_graph";
const REPLAY_BATCH_SIZE: i64 = 128;
const REPLAY_EVENT_TIMEOUT: Duration = Duration::from_secs(10);
const REPLAY_IDLE_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct XanaduReplayPayload {
    merged_group_id: Option<Uuid>,
    transclusion_id: Uuid,
}
