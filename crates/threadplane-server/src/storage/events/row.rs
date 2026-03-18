use super::*;
use core::str::FromStr as _;

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
