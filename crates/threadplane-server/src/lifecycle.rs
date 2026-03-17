#![expect(
    clippy::redundant_pub_crate,
    reason = "Lifecycle helpers are crate-local utilities with explicit visibility."
)]

use chrono::{DateTime, Duration, Utc};
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub(crate) const MINIMUM_LEASE_SECONDS: i64 = 30;

#[inline]
#[must_use]
pub(crate) fn calculate_claim_expiry(
    claimed_at: DateTime<Utc>,
    lease_seconds: i64,
) -> Option<DateTime<Utc>> {
    claimed_at.checked_add_signed(Duration::seconds(lease_seconds))
}

#[inline]
#[must_use]
pub(crate) fn normalized_lease_seconds(
    requested_lease_seconds: Option<i64>,
    default_lease_seconds: i64,
) -> i64 {
    requested_lease_seconds
        .unwrap_or(default_lease_seconds)
        .max(MINIMUM_LEASE_SECONDS)
}

pub(crate) async fn wait_for_shutdown(shutdown_token: CancellationToken) {
    shutdown_token.cancelled().await;
}

pub(crate) async fn watch_for_shutdown_signal(shutdown_token: CancellationToken) {
    match signal::ctrl_c().await {
        Ok(()) => info!("shutdown signal received"),
        Err(error) => error!(
            ?error,
            "failed to listen for shutdown signal; cancelling runtime"
        ),
    }

    shutdown_token.cancel();
}
