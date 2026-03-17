#![expect(
    clippy::redundant_pub_crate,
    reason = "Idempotency helpers are crate-local orchestration around command receipts."
)]

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlx::{query_as, Postgres, Transaction};
use uuid::Uuid;

use crate::error::{ServerResult, ThreadplaneServerError};
use threadplane_core::{ApiEnvelope, CommandReceipt};

pub(crate) const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
const COMMAND_STATUS_COMPLETED: &str = "completed";
const COMMAND_STATUS_PENDING: &str = "pending";
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
    command_id: Uuid,
    command_kind: String,
    idempotency_key: String,
    recorded_at: DateTime<Utc>,
}

impl PendingCommandReceipt {
    fn as_receipt(&self, replayed: bool) -> CommandReceipt {
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
struct CommandReceiptRow {
    command_id: Uuid,
    command_kind: String,
    idempotency_key: String,
    recorded_at: DateTime<Utc>,
    request_payload: Value,
    response_payload: Option<Value>,
    status: String,
}

pub(crate) async fn begin_idempotent_command<T: DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    context: IdempotencyContext<'_>,
    now: DateTime<Utc>,
) -> ServerResult<CommandExecution<T>> {
    let Some(raw_key) = context.idempotency_key else {
        return Ok(CommandExecution::Execute(None));
    };

    let idempotency_key = normalize_idempotency_key(raw_key)?;
    let existing_row: Option<CommandReceiptRow> = query_as(
        "
        SELECT
            command_id,
            command_kind,
            idempotency_key,
            request_payload,
            response_payload,
            status,
            recorded_at
        FROM command_receipts
        WHERE workspace = $1
          AND actor = $2
          AND command_kind = $3
          AND idempotency_key = $4
        FOR UPDATE
        ",
    )
    .bind(context.workspace)
    .bind(context.actor)
    .bind(context.command_kind)
    .bind(&idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(command_receipt_row) = existing_row {
        return replay_or_reject_existing_command(
            tx,
            command_receipt_row,
            context.request_payload,
            now,
        )
            .await;
    }

    let command_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO command_receipts (
            command_id,
            workspace,
            actor,
            command_kind,
            idempotency_key,
            request_payload,
            response_payload,
            status,
            recorded_at,
            last_seen_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $8, $8)
        ",
    )
    .bind(command_id)
    .bind(context.workspace)
    .bind(context.actor)
    .bind(context.command_kind)
    .bind(&idempotency_key)
    .bind(context.request_payload.clone())
    .bind(COMMAND_STATUS_PENDING)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    Ok(CommandExecution::Execute(Some(PendingCommandReceipt {
        command_id,
        command_kind: context.command_kind.to_owned(),
        idempotency_key,
        recorded_at: now,
    })))
}

pub(crate) async fn complete_idempotent_command<T: Serialize + Sync>(
    tx: &mut Transaction<'_, Postgres>,
    pending_receipt: Option<&PendingCommandReceipt>,
    data: &T,
    now: DateTime<Utc>,
) -> ServerResult<Option<CommandReceipt>> {
    let Some(pending_command_receipt) = pending_receipt else {
        return Ok(None);
    };

    let receipt = pending_command_receipt.as_receipt(false);
    let response_payload = serde_json::to_value(ApiEnvelope {
        data,
        ok: true,
        receipt: Some(receipt.clone()),
    })?;

    sqlx::query(
        "
        UPDATE command_receipts
        SET response_payload = $2,
            status = $3,
            last_seen_at = $4
        WHERE command_id = $1
        ",
    )
    .bind(pending_command_receipt.command_id)
    .bind(response_payload)
    .bind(COMMAND_STATUS_COMPLETED)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    Ok(Some(receipt))
}

async fn replay_or_reject_existing_command<T: DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    existing: CommandReceiptRow,
    request_payload: &Value,
    now: DateTime<Utc>,
) -> ServerResult<CommandExecution<T>> {
    if existing.request_payload != *request_payload {
        return Err(ThreadplaneServerError::conflict(
            "idempotency key was already used with a different payload",
        ));
    }

    sqlx::query(
        "
        UPDATE command_receipts
        SET last_seen_at = $2
        WHERE command_id = $1
        ",
    )
    .bind(existing.command_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    if existing.status != COMMAND_STATUS_COMPLETED {
        return Err(ThreadplaneServerError::conflict(
            "command with this idempotency key is already in progress",
        ));
    }

    let response_payload = existing.response_payload.ok_or_else(|| {
        ThreadplaneServerError::internal("completed command receipt is missing a response payload")
    })?;
    let mut envelope: ApiEnvelope<T> = serde_json::from_value(response_payload)?;
    envelope.receipt = Some(
        PendingCommandReceipt {
            command_id: existing.command_id,
            command_kind: existing.command_kind,
            idempotency_key: existing.idempotency_key,
            recorded_at: existing.recorded_at,
        }
        .as_receipt(true),
    );

    Ok(CommandExecution::Replay(envelope))
}

fn normalize_idempotency_key(raw_key: &str) -> ServerResult<String> {
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
