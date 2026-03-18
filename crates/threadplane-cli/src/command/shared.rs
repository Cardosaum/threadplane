#![allow(
    clippy::wildcard_imports,
    reason = "Shared command helpers intentionally build on the command module prelude."
)]

use super::*;

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum OutputFormat {
    Compact,
    #[default]
    Json,
}

#[derive(Clone, Copy)]
pub(crate) struct MemoryListPathArgs<'input> {
    pub(crate) audience: Option<&'input str>,
    pub(crate) importance: Option<&'input str>,
    pub(crate) kind: Option<&'input str>,
    pub(crate) limit: Option<i64>,
    pub(crate) query: Option<&'input str>,
    pub(crate) recall_trigger: Option<&'input str>,
    pub(crate) tag: Option<&'input str>,
    pub(crate) workspace: &'input str,
}

pub(crate) fn build_mismatch_warning(comparison: &BuildComparison) -> Option<String> {
    if comparison.matches {
        return None;
    }

    let changed_fields = comparison
        .differences
        .iter()
        .map(|difference| difference.field.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    Some(format!(
        "threadplane-cli {} ({}) differs from server {} ({}); changed fields: {}. Run `threadplane build compare` for details.",
        comparison.client.version,
        comparison.client.git_commit.as_deref().unwrap_or("unknown"),
        comparison.server.version,
        comparison.server.git_commit.as_deref().unwrap_or("unknown"),
        changed_fields,
    ))
}
