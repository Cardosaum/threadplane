#![allow(
    clippy::wildcard_imports,
    reason = "Record submodule reuses shared type imports via the crate-local prelude style"
)]

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub entity_ref: String,
    pub event_id: Uuid,
    pub note_id: Uuid,
    pub title: String,
    pub transclusion_id: Option<Uuid>,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Display, Serialize, Deserialize)]
#[display("{_0}")]
#[serde(transparent)]
pub struct MemoryKind(String);

impl MemoryKind {
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline]
    #[must_use]
    pub fn from_lossy<T>(value: T) -> Self
    where
        T: Into<String>,
    {
        Self::new(value).unwrap_or_else(|| Self("memory".to_owned()))
    }

    #[inline]
    #[must_use]
    pub fn new<T>(value: T) -> Option<Self>
    where
        T: Into<String>,
    {
        let normalized = normalize_memory_kind_name(&value.into());
        (!normalized.is_empty()).then_some(Self(normalized))
    }
}

impl FromStr for MemoryKind {
    type Err = &'static str;

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::new(input).ok_or("memory kind cannot be empty")
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MemoryAudience {
    Agent,
    Both,
    Human,
}

impl MemoryAudience {
    #[inline]
    #[must_use]
    pub fn includes(self, requested: Self) -> bool {
        matches!(self, Self::Both) || self == requested
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MemoryImportance {
    Critical,
    High,
    Normal,
}

impl MemoryImportance {
    #[inline]
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Normal => 10,
            Self::High => 20,
            Self::Critical => 30,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MemoryScope {
    Global,
    Repo,
    Workspace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub audience: MemoryAudience,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub entity_ref: String,
    pub event_id: Uuid,
    pub importance: MemoryImportance,
    pub kind: MemoryKind,
    pub memory_id: Uuid,
    pub recall_triggers: Vec<String>,
    pub scope: MemoryScope,
    pub tags: Vec<String>,
    pub title: String,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpicRecord {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub entity_ref: String,
    pub epic_id: Uuid,
    pub event_id: Uuid,
    pub title: String,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRecord {
    pub actor: String,
    pub created_at: String,
    pub event_id: Uuid,
    pub from: String,
    pub is_xanadu: bool,
    pub link_id: Uuid,
    pub relation: String,
    pub to: String,
    pub transclusion_id: Option<Uuid>,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub actor: String,
    pub created_at: String,
    pub event_id: Uuid,
    pub kind: EventKind,
    pub payload: Value,
    pub workspace: String,
}

#[inline]
#[must_use]
pub fn normalize_memory_kind_name(name: &str) -> String {
    normalize_identifier(name)
}

#[inline]
#[must_use]
pub fn normalize_memory_tags(tags: Vec<String>) -> Vec<String> {
    normalize_identifier_list(tags)
}

#[inline]
#[must_use]
pub fn normalize_memory_recall_triggers(triggers: Vec<String>) -> Vec<String> {
    normalize_identifier_list(triggers)
}

fn normalize_identifier(input: &str) -> String {
    relation_type(input).to_ascii_lowercase()
}

fn normalize_identifier_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalize_identifier(&value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}
