use super::super::*;

#[derive(Debug, Args, Clone)]
pub(crate) struct TaskMetadataArgs {
    #[arg(long, help = "Durable label. Repeat for multiple labels")]
    pub(crate) label: Vec<String>,

    #[arg(long, help = "Durable owner, distinct from the temporary claim actor")]
    pub(crate) owner: Option<String>,

    #[arg(long, help = "Priority used for backlog sorting and filtering")]
    pub(crate) priority: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct TaskMetadataFilterArgs {
    #[arg(long, help = "Only include tasks owned by this durable owner")]
    pub(crate) owner: Option<String>,

    #[arg(long, help = "Only include tasks with this priority")]
    pub(crate) priority: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
pub(crate) struct TaskMetadataPatchArgs {
    #[arg(long, help = "Clear all durable labels")]
    pub(crate) clear_labels: bool,

    #[arg(long, help = "Clear any durable owner")]
    pub(crate) clear_owner: bool,

    #[arg(
        long,
        help = "Replace labels with this set. Repeat for multiple labels"
    )]
    pub(crate) label: Vec<String>,

    #[arg(long, help = "Replace the durable owner")]
    pub(crate) owner: Option<String>,

    #[arg(long, help = "Replace the task priority")]
    pub(crate) priority: Option<String>,
}
