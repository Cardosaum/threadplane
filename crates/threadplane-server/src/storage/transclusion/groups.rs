use super::*;

pub(super) async fn fetch_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<TransclusionGroupRow> {
    query_as(
        "
        SELECT transclusion_id, workspace, title, content, created_at, updated_at
        FROM transclusion_groups
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("transclusion group not found"))
}

pub(super) async fn group_exists(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<bool> {
    let exists: Option<(Uuid,)> =
        query_as("SELECT transclusion_id FROM transclusion_groups WHERE transclusion_id = $1")
            .bind(transclusion_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(exists.is_some())
}

pub(super) async fn insert_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        INSERT INTO transclusion_groups (
            transclusion_id,
            workspace,
            created_by,
            title,
            content,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $6)
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(actor)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
