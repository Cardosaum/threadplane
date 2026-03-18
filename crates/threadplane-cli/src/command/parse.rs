#![allow(
    clippy::wildcard_imports,
    reason = "Parse helpers intentionally build on the command module prelude."
)]

use super::*;

pub(crate) fn parse_memory_kind_input(input: &str) -> Result<MemoryKind> {
    MemoryKind::new(input).ok_or_else(|| {
        Usage {
            message: "memory kind cannot be empty".to_owned(),
        }
        .build()
    })
}

pub(crate) fn parse_memory_audience_input(input: &str) -> Result<MemoryAudience> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported memory audience `{input}`"),
        }
        .build()
    })
}

pub(crate) fn parse_memory_importance_input(input: &str) -> Result<MemoryImportance> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported memory importance `{input}`"),
        }
        .build()
    })
}

pub(crate) fn parse_memory_scope_input(input: &str) -> Result<MemoryScope> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported memory scope `{input}`"),
        }
        .build()
    })
}

pub(crate) fn normalize_memory_filter_name(input: &str) -> Result<String> {
    let normalized = threadplane_core::normalize_memory_kind_name(input);
    if normalized.is_empty() {
        return Err(Usage {
            message: "memory filters cannot be empty".to_owned(),
        }
        .build());
    }

    Ok(normalized)
}

pub(crate) fn parse_public_key_algorithm(
    input: &str,
) -> Result<threadplane_core::PublicKeyAlgorithm> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported public-key algorithm `{input}`"),
        }
        .build()
    })
}

pub(crate) fn parse_public_key_algorithms(
    inputs: &[String],
) -> Result<Vec<threadplane_core::PublicKeyAlgorithm>> {
    inputs
        .iter()
        .map(String::as_str)
        .map(parse_public_key_algorithm)
        .collect()
}

pub(crate) fn parse_workspace_priority_specs(inputs: &[String]) -> Result<Vec<WorkspacePriority>> {
    inputs
        .iter()
        .map(String::as_str)
        .map(parse_workspace_priority_spec)
        .collect()
}

pub(crate) fn parse_workspace_role(input: &str) -> Result<WorkspaceRole> {
    input.parse().map_err(|_error| {
        Usage {
            message: format!("unsupported workspace role `{input}`"),
        }
        .build()
    })
}

fn parse_workspace_priority_spec(input: &str) -> Result<WorkspacePriority> {
    let mut parts = input.splitn(3, ':');
    let raw_name = parts.next().unwrap_or_default();
    let raw_rank = parts.next().ok_or_else(|| {
        Usage {
            message: format!(
                "priority definition `{input}` must look like name:rank[:description]"
            ),
        }
        .build()
    })?;
    let description = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let rank = raw_rank.parse::<u16>().map_err(|_error| {
        Usage {
            message: format!("priority rank `{raw_rank}` must be an unsigned integer"),
        }
        .build()
    })?;

    Ok(WorkspacePriority {
        description,
        name: normalize_priority_name(raw_name)?,
        rank,
    })
}
