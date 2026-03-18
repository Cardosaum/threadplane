use super::*;

pub(crate) async fn fetch_entity_record(
    pool: &PgPool,
    entity_ref: &str,
) -> ServerResult<EntityRecord> {
    match parse_entity_ref(entity_ref) {
        Some(EntityRef::Epic(epic_id)) => {
            let epic = fetch_epic_by_id(pool, epic_id).await?;
            Ok(EntityRecord::Epic(EpicRecord::from(epic)))
        }
        Some(EntityRef::Memory(memory_id)) => {
            let memory = fetch_memory_by_id(pool, memory_id).await?;
            Ok(EntityRecord::Memory(MemoryRecord::try_from(memory)?))
        }
        Some(EntityRef::Note(note_id)) => {
            let note = fetch_note_by_id(pool, note_id).await?;
            Ok(EntityRecord::Note(NoteRecord::from(note)))
        }
        Some(EntityRef::Task(task_id)) => {
            let task = fetch_task_by_id(pool, task_id).await?;
            Ok(EntityRecord::Task(TaskRecord::from(task)))
        }
        None => Err(ThreadplaneServerError::bad_request(format!(
            "invalid entity ref: {entity_ref}"
        ))),
    }
}
