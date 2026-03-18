use tokio::{task::JoinHandle, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::{
    batch::replay_graph_projection_batch, AppState, GRAPH_PROJECTION_NAME, REPLAY_BATCH_SIZE,
    REPLAY_IDLE_POLL_INTERVAL,
};

pub(crate) fn spawn_graph_projection_worker(
    state: AppState,
    shutdown_token: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match Box::pin(replay_graph_projection_batch(&state, REPLAY_BATCH_SIZE)).await {
                Ok(0) => {
                    tokio::select! {
                        () = shutdown_token.cancelled() => break,
                        () = sleep(REPLAY_IDLE_POLL_INTERVAL) => {},
                    }
                }
                Ok(replayed) => info!(
                    projection = GRAPH_PROJECTION_NAME,
                    replayed, "replayed graph projection batch"
                ),
                Err(error) => {
                    error!(
                        ?error,
                        projection = GRAPH_PROJECTION_NAME,
                        "graph projection replay failed"
                    );
                    tokio::select! {
                        () = shutdown_token.cancelled() => break,
                        () = sleep(REPLAY_IDLE_POLL_INTERVAL) => {},
                    }
                }
            }
        }
    })
}
