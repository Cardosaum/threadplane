use super::*;
use crate::storage::transclusion::{
    entities::fetch_text_entity_by_ref_tx,
    groups::{fetch_transclusion_group, group_exists, insert_transclusion_group},
};

pub(crate) async fn update_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    _actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        UPDATE transclusion_groups
        SET workspace = $2,
            title = $3,
            content = $4,
            updated_at = $5
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn sync_transclusion_members(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    let group = fetch_transclusion_group(tx, transclusion_id).await?;

    sqlx::query(
        "
        UPDATE notes
        SET title = $2,
            body = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET title = $2,
            details = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub(crate) async fn prepare_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &threadplane_core::CreateXanaduLinkRequest,
    created_at: DateTime<Utc>,
) -> ServerResult<XanaduGroup> {
    let from = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.from).await?;
    let to = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.to).await?;
    let canonical_group_id = from
        .transclusion_id()
        .or_else(|| to.transclusion_id())
        .unwrap_or_else(Uuid::new_v4);
    let merged_group_id = match (from.transclusion_id(), to.transclusion_id()) {
        (Some(left), Some(right)) if left != right => Some(right),
        _ => None,
    };

    upsert_xanadu_group(
        tx,
        request,
        &from,
        canonical_group_id,
        merged_group_id,
        created_at,
    )
    .await?;
    set_entity_transclusion(tx, &from, canonical_group_id).await?;
    set_entity_transclusion(tx, &to, canonical_group_id).await?;
    sync_transclusion_members(tx, canonical_group_id).await?;

    Ok(XanaduGroup {
        canonical_group_id,
        merged_group_id,
    })
}

async fn move_group_members(
    tx: &mut Transaction<'_, Postgres>,
    from_group_id: Uuid,
    to_group_id: Uuid,
) -> ServerResult<()> {
    sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn set_entity_transclusion(
    tx: &mut Transaction<'_, Postgres>,
    entity: &TextEntityRow,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    match entity {
        TextEntityRow::Note(note) => {
            sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE note_id = $1")
                .bind(note.note_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
        TextEntityRow::Task(task) => {
            sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE task_id = $1")
                .bind(task.task_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

async fn upsert_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &threadplane_core::CreateXanaduLinkRequest,
    from: &TextEntityRow,
    canonical_group_id: Uuid,
    merged_group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) -> ServerResult<()> {
    let source_title = from.title().to_owned();
    let source_content = from.content().to_owned();

    if group_exists(tx, canonical_group_id).await? {
        update_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    } else {
        insert_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    }

    if let Some(group_id) = merged_group_id {
        move_group_members(tx, group_id, canonical_group_id).await?;
        sqlx::query("DELETE FROM transclusion_groups WHERE transclusion_id = $1")
            .bind(group_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}
