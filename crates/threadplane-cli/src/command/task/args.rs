use super::super::*;
use alloc::collections::BTreeSet;

#[derive(Debug, Args)]
#[command(about = "Offer, claim, and inspect shared tasks")]
pub(crate) struct TaskCommand {
    #[command(subcommand)]
    pub(crate) command: TaskSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TaskSubcommand {
    #[command(about = "Show tasks blocking the selected task")]
    BlockedBy(TaskDependencyViewCommand),
    #[command(about = "Show tasks that are blocked by the selected task")]
    Blocks(TaskDependencyViewCommand),
    #[command(about = "Claim an open task with a lease")]
    Claim(ClaimTask),
    #[command(about = "Claim the next best ready task in the workspace")]
    ClaimNext(ClaimNextTask),
    #[command(about = "Mark a task complete and release any active claim")]
    Complete(CompleteTask),
    #[command(about = "Fetch a task plus graph-backed related context")]
    Context(TaskContextCommand),
    #[command(about = "Show the task dependency DAG around a task")]
    Dag(TaskDagCommand),
    #[command(about = "Declare that one task depends on another")]
    Depend(AddTaskDependency),
    #[command(about = "List tasks with workflow filters")]
    List(ListTasks),
    #[command(about = "Show the next best ready task in the workspace")]
    Next(NextTask),
    #[command(about = "Offer a new task into a workspace")]
    Offer(OfferTask),
    #[command(about = "Release an active claim and return the task to the pool")]
    Release(ReleaseTask),
    #[command(about = "Fetch a task by ID without graph context")]
    Show(ShowTask),
    #[command(about = "Apply the same epic assignment and/or completion to multiple tasks")]
    Triage(TriageTasks),
    #[command(about = "Update a task and propagate through Xanadu links when present")]
    Update(UpdateTask),
}

#[derive(Debug, Args)]
pub(crate) struct ClaimTask {
    #[arg(long, help = "Actor claiming the task")]
    pub(crate) actor: String,

    #[arg(long, help = "Lease duration in seconds")]
    pub(crate) lease_seconds: Option<i64>,

    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ClaimNextTask {
    #[arg(long, help = "Actor claiming the task")]
    pub(crate) actor: String,

    #[arg(long, help = "Optional epic filter")]
    pub(crate) epic_id: Option<Uuid>,

    #[arg(long, help = "Durable label filter")]
    pub(crate) label: Option<String>,

    #[arg(long, help = "Lease duration in seconds")]
    pub(crate) lease_seconds: Option<i64>,

    #[command(flatten)]
    pub(crate) metadata_filters: TaskMetadataFilterArgs,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct TaskDependencyViewCommand {
    #[arg(
        long,
        help = "Only return direct relationships instead of the transitive chain"
    )]
    pub(crate) direct_only: bool,

    #[arg(
        long,
        default_value = "compact",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,
}

#[derive(Debug, Args)]
pub(crate) struct CompleteTask {
    #[arg(long, help = "Actor completing the task")]
    pub(crate) actor: String,

    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct TaskContextCommand {
    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,
}

#[derive(Debug, Args)]
pub(crate) struct TaskDagCommand {
    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum TaskStatusValue {
    Claimed,
    Completed,
    Open,
}

impl TaskStatusValue {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Claimed => "claimed",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct AddTaskDependency {
    #[arg(long, help = "Actor adding the dependency edge")]
    pub(crate) actor: String,

    #[arg(long, help = "Task UUID that must complete first")]
    pub(crate) depends_on: Uuid,

    #[arg(long, help = "Task UUID that will wait")]
    pub(crate) task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListTasks {
    #[arg(long, help = "Optional epic filter")]
    pub(crate) epic_id: Option<Uuid>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(long, help = "Only include tasks with the selected normalized label")]
    pub(crate) label: Option<String>,

    #[arg(long, help = "Maximum number of tasks to return")]
    pub(crate) limit: Option<i64>,

    #[command(flatten)]
    pub(crate) metadata_filters: TaskMetadataFilterArgs,

    #[arg(long, help = "Only include tasks whose dependencies are all completed")]
    pub(crate) ready_only: bool,

    #[arg(long, help = "Optional workflow status filter")]
    pub(crate) status: Option<TaskStatusValue>,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct NextTask {
    #[arg(long, help = "Optional epic filter")]
    pub(crate) epic_id: Option<Uuid>,

    #[arg(
        long,
        default_value = "json",
        help = "Render JSON or a compact human-readable summary"
    )]
    pub(crate) format: OutputFormat,

    #[arg(long, help = "Durable label filter")]
    pub(crate) label: Option<String>,

    #[command(flatten)]
    pub(crate) metadata_filters: TaskMetadataFilterArgs,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

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

#[derive(Debug, Args)]
pub(crate) struct OfferTask {
    #[arg(long, help = "Task author")]
    pub(crate) author: String,

    #[arg(long, help = "Dependency task UUID. Repeat for multiple dependencies")]
    pub(crate) depends_on: Vec<Uuid>,

    #[arg(long, help = "Task details")]
    pub(crate) details: String,

    #[arg(long, help = "Optional epic UUID to attach this task to")]
    pub(crate) epic_id: Option<Uuid>,

    #[command(flatten)]
    pub(crate) metadata: TaskMetadataArgs,

    #[arg(long, help = "Task title")]
    pub(crate) title: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReleaseTask {
    #[arg(long, help = "Actor releasing the task")]
    pub(crate) actor: String,

    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct ShowTask {
    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,
}

#[derive(Debug, Args)]
pub(crate) struct TriageTasks {
    #[arg(long, help = "Actor performing the triage")]
    pub(crate) actor: String,

    #[arg(
        long,
        help = "Mark every listed task completed after any metadata updates"
    )]
    pub(crate) complete: bool,

    #[arg(long, help = "Optional epic UUID to assign to every listed task")]
    pub(crate) epic_id: Option<Uuid>,

    #[command(flatten)]
    pub(crate) metadata: TaskMetadataPatchArgs,

    #[arg(
        long,
        help = "Task UUID to triage. Repeat for multiple tasks",
        num_args = 1..,
        required = true
    )]
    pub(crate) task_id: Vec<Uuid>,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateTask {
    #[arg(long, help = "Actor performing the update")]
    pub(crate) actor: String,

    #[arg(long, help = "Updated task details")]
    pub(crate) details: String,

    #[arg(
        long,
        help = "Optional epic UUID. When provided, the task is attached to that epic"
    )]
    pub(crate) epic_id: Option<Uuid>,

    #[command(flatten)]
    pub(crate) metadata: TaskMetadataArgs,

    #[arg(long, help = "Task UUID")]
    pub(crate) task_id: Uuid,

    #[arg(long, help = "Updated task title")]
    pub(crate) title: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TaskTriageSummary {
    pub(crate) clear_labels: bool,
    pub(crate) clear_owner: bool,
    pub(crate) completed_task_ids: Vec<Uuid>,
    pub(crate) epic_id: Option<Uuid>,
    pub(crate) labels: Option<Vec<String>>,
    pub(crate) owner: Option<String>,
    pub(crate) priority: Option<TaskPriority>,
    pub(crate) task_ids: Vec<Uuid>,
    pub(crate) unchanged_task_ids: Vec<Uuid>,
    pub(crate) updated_task_ids: Vec<Uuid>,
    pub(crate) workspace: String,
}

#[derive(Debug, Default)]
pub(crate) struct TaskTriageOutcome {
    pub(crate) changed: bool,
    pub(crate) completed: bool,
    pub(crate) updated: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TaskDependencyViewKind {
    BlockedBy,
    Blocks,
}

pub(crate) fn dedup_task_ids(task_ids: &[Uuid]) -> Vec<Uuid> {
    task_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
