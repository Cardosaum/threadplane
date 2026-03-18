use super::*;

pub(super) fn normalized_text_query(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(str::to_owned)
}

pub(super) fn normalized_memory_tag_filter(value: Option<&str>) -> Option<String> {
    normalize_memory_tags(value.map(str::to_owned).into_iter().collect())
        .into_iter()
        .next()
}

pub(super) fn normalized_memory_recall_trigger_filter(value: Option<&str>) -> Option<String> {
    normalize_memory_recall_triggers(value.map(str::to_owned).into_iter().collect())
        .into_iter()
        .next()
}
