use super::{NoteRow, TaskRow, Uuid};

pub(crate) enum TextEntityRow {
    Note(NoteRow),
    Task(TaskRow),
}

impl TextEntityRow {
    pub(crate) const fn transclusion_id(&self) -> Option<Uuid> {
        match self {
            Self::Note(note) => note.transclusion_id,
            Self::Task(task) => task.transclusion_id,
        }
    }

    pub(crate) fn title(&self) -> &str {
        match self {
            Self::Note(note) => &note.title,
            Self::Task(task) => &task.title,
        }
    }

    pub(crate) fn content(&self) -> &str {
        match self {
            Self::Note(note) => &note.body,
            Self::Task(task) => &task.details,
        }
    }
}
