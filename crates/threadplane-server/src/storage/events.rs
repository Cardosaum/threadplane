#![expect(
    clippy::redundant_pub_crate,
    reason = "Event log persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The event log submodule intentionally builds on the storage prelude."
)]

use super::*;

pub(crate) async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    actor: &str,
    kind: EventKind,
    payload: &Value,
    created_at: DateTime<Utc>,
) -> ServerResult<Uuid> {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(event_id)
    .bind(workspace)
    .bind(actor)
    .bind(event_kind_name(kind))
    .bind(payload.clone())
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(event_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectionCursor {
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) event_id: Uuid,
}

impl ProjectionCursor {
    #[must_use]
    pub(crate) const fn new(created_at: DateTime<Utc>, event_id: Uuid) -> Self {
        Self {
            created_at,
            event_id,
        }
    }
}

pub(crate) async fn fetch_event_rows_for_workspace(
    pool: &PgPool,
    workspace: &str,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(workspace)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_event_row_for_workspace(
    pool: &PgPool,
    workspace: &str,
    event_id: Uuid,
) -> ServerResult<EventRow> {
    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
          AND event_id = $2
        ",
    )
    .bind(workspace)
    .bind(event_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("event not found"))
}

pub(crate) async fn fetch_event_rows_after_workspace_cursor(
    pool: &PgPool,
    workspace: &str,
    cursor: Option<ProjectionCursor>,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    if let Some(current_cursor) = cursor {
        return query_as(
            "
            SELECT event_id, workspace, actor, kind, payload, created_at
            FROM events
            WHERE workspace = $1
              AND (
                    created_at > $2
                 OR (created_at = $2 AND event_id > $3)
              )
            ORDER BY created_at ASC, event_id ASC
            LIMIT $4
            ",
        )
        .bind(workspace)
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(Into::into);
    }

    let mut rows: Vec<EventRow> = query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC, event_id DESC
        LIMIT $2
        ",
    )
    .bind(workspace)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

pub(crate) async fn fetch_event_rows_after_cursor(
    pool: &PgPool,
    cursor: Option<ProjectionCursor>,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    if let Some(current_cursor) = cursor {
        return query_as(
            "
            SELECT event_id, workspace, actor, kind, payload, created_at
            FROM events
            WHERE created_at > $1
               OR (created_at = $1 AND event_id > $2)
            ORDER BY created_at ASC, event_id ASC
            LIMIT $3
            ",
        )
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(Into::into);
    }

    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        ORDER BY created_at ASC, event_id ASC
        LIMIT $1
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_projection_cursor(
    pool: &PgPool,
    projection_name: &str,
) -> ServerResult<Option<ProjectionCursor>> {
    let row: Option<ProjectionOffsetRow> = query_as(
        "
        SELECT last_event_created_at, last_event_id
        FROM projection_offsets
        WHERE projection_name = $1
        ",
    )
    .bind(projection_name)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(ProjectionOffsetRow::into_cursor))
}

pub(crate) async fn record_projection_cursor(
    tx: &mut Transaction<'_, Postgres>,
    projection_name: &str,
    cursor: ProjectionCursor,
    updated_at: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        INSERT INTO projection_offsets (
            projection_name,
            last_event_created_at,
            last_event_id,
            updated_at
        )
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (projection_name) DO UPDATE
        SET last_event_created_at = EXCLUDED.last_event_created_at,
            last_event_id = EXCLUDED.last_event_id,
            updated_at = EXCLUDED.updated_at
        ",
    )
    .bind(projection_name)
    .bind(cursor.created_at)
    .bind(cursor.event_id)
    .bind(updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub(crate) async fn fetch_projection_status(
    pool: &PgPool,
    projection_name: &str,
) -> ServerResult<ProjectionStatus> {
    let cursor = fetch_projection_cursor(pool, projection_name).await?;
    let total_events = count_all_events(pool).await?;
    let pending_events = count_events_after_cursor(pool, cursor).await?;

    Ok(build_projection_status(
        projection_name,
        cursor,
        total_events,
        pending_events,
    ))
}

async fn count_all_events(pool: &PgPool) -> ServerResult<i64> {
    let (count,): (i64,) = query_as("SELECT COUNT(*) FROM events")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

async fn count_events_after_cursor(
    pool: &PgPool,
    cursor: Option<ProjectionCursor>,
) -> ServerResult<i64> {
    if let Some(current_cursor) = cursor {
        let (count,): (i64,) = query_as(
            "
            SELECT COUNT(*)
            FROM events
            WHERE created_at > $1
               OR (created_at = $1 AND event_id > $2)
            ",
        )
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .fetch_one(pool)
        .await?;
        return Ok(count);
    }

    count_all_events(pool).await
}

pub(crate) fn build_projection_status(
    projection_name: &str,
    cursor: Option<ProjectionCursor>,
    total_events: i64,
    pending_events: i64,
) -> ProjectionStatus {
    let projected_events = total_events.saturating_sub(pending_events);
    let (last_event_created_at, last_event_id) = cursor.map_or((None, None), |current_cursor| {
        (
            Some(current_cursor.created_at.to_rfc3339()),
            Some(current_cursor.event_id),
        )
    });

    ProjectionStatus {
        caught_up: pending_events == 0,
        last_event_created_at,
        last_event_id,
        pending_events,
        projected_events,
        projection_name: projection_name.to_owned(),
        total_events,
    }
}

pub(crate) fn event_kind_name(kind: EventKind) -> String {
    kind.to_string()
}

pub(crate) fn parse_event_kind(value: &str) -> EventKind {
    EventKind::from_str(value).unwrap_or(EventKind::NoteRecorded)
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct EventRow {
    pub(crate) event_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) actor: String,
    pub(crate) kind: String,
    pub(crate) payload: Value,
    pub(crate) created_at: DateTime<Utc>,
}

impl EventRow {
    #[must_use]
    pub(crate) const fn cursor(&self) -> ProjectionCursor {
        ProjectionCursor::new(self.created_at, self.event_id)
    }

    #[must_use]
    pub(crate) fn parsed_kind(&self) -> EventKind {
        parse_event_kind(&self.kind)
    }
}

#[derive(Debug, FromRow)]
struct ProjectionOffsetRow {
    last_event_created_at: Option<DateTime<Utc>>,
    last_event_id: Option<Uuid>,
}

impl ProjectionOffsetRow {
    const fn into_cursor(self) -> Option<ProjectionCursor> {
        match (self.last_event_created_at, self.last_event_id) {
            (Some(created_at), Some(event_id)) => Some(ProjectionCursor::new(created_at, event_id)),
            _ => None,
        }
    }
}

impl From<EventRow> for EventRecord {
    #[inline]
    fn from(value: EventRow) -> Self {
        Self {
            event_id: value.event_id,
            workspace: value.workspace,
            actor: value.actor,
            kind: parse_event_kind(&value.kind),
            payload: value.payload,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}
