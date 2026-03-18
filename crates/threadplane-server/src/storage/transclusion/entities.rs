use super::*;

pub(super) async fn fetch_text_entity_by_ref_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    entity_ref: &str,
) -> ServerResult<TextEntityRow> {
    match parse_entity_ref(entity_ref) {
        Some(EntityRef::Epic(_) | EntityRef::Memory(_)) => {
            Err(ThreadplaneServerError::bad_request(format!(
                "non-textual entity refs cannot join xanadu groups: {entity_ref}"
            )))
        }
        Some(EntityRef::Note(note_id)) => Ok(TextEntityRow::Note(
            fetch_note_by_id_tx(tx, note_id, workspace).await?,
        )),
        Some(EntityRef::Task(task_id)) => Ok(TextEntityRow::Task(
            fetch_task_by_id_tx(tx, task_id, workspace).await?,
        )),
        None => Err(ThreadplaneServerError::bad_request(format!(
            "unsupported entity ref {entity_ref}"
        ))),
    }
}
