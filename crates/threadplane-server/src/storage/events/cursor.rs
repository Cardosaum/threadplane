use super::*;

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
