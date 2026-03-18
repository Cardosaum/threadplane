use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{ServerResult, ThreadplaneServerError};
use threadplane_core::{ApiEnvelope, CommandReceipt};

pub(crate) const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
pub(crate) const COMMAND_STATUS_COMPLETED: &str = "completed";
pub(crate) const COMMAND_STATUS_PENDING: &str = "pending";
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 255;

pub(crate) struct IdempotencyContext<'context> {
    pub(crate) actor: &'context str,
    pub(crate) command_kind: &'context str,
    pub(crate) idempotency_key: Option<&'context str>,
    pub(crate) request_payload: &'context Value,
    pub(crate) workspace: &'context str,
}

pub(crate) enum CommandExecution<T> {
    Execute(Option<PendingCommandReceipt>),
    Replay(ApiEnvelope<T>),
}

pub(crate) struct PendingCommandReceipt {
    pub(crate) command_id: Uuid,
    pub(crate) command_kind: String,
    pub(crate) idempotency_key: String,
    pub(crate) recorded_at: DateTime<Utc>,
}

impl PendingCommandReceipt {
    pub(crate) fn as_receipt(&self, replayed: bool) -> CommandReceipt {
        CommandReceipt {
            command_id: self.command_id,
            command_kind: self.command_kind.clone(),
            idempotency_key: self.idempotency_key.clone(),
            recorded_at: self.recorded_at.to_rfc3339(),
            replayed,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct CommandReceiptRow {
    pub(crate) command_id: Uuid,
    pub(crate) command_kind: String,
    pub(crate) idempotency_key: String,
    pub(crate) recorded_at: DateTime<Utc>,
    pub(crate) request_payload: Value,
    pub(crate) response_payload: Option<Value>,
    pub(crate) status: String,
}

pub(crate) fn normalize_idempotency_key(raw_key: &str) -> ServerResult<String> {
    let trimmed_key = raw_key.trim();
    if trimmed_key.is_empty() {
        return Err(ThreadplaneServerError::bad_request(
            "idempotency key must not be empty",
        ));
    }
    if trimmed_key.len() > MAX_IDEMPOTENCY_KEY_LENGTH {
        return Err(ThreadplaneServerError::bad_request(
            "idempotency key is too long",
        ));
    }

    Ok(trimmed_key.to_owned())
}
