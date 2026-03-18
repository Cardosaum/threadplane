#![expect(
    clippy::arbitrary_source_item_ordering,
    clippy::redundant_pub_crate,
    reason = "Executor keeps command flow grouped by surface area and uses command-module boundaries as its import seam."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "Executor uses the command module as its import boundary to keep the orchestration layer concise."
)]

use super::paths::*;
use super::render::*;
use super::*;

mod content;
mod reads;
mod workspace;

pub(crate) fn execute<'cfg, 'ctx, A, O, S>(
    cli: Cli,
    config: &'cfg ThreadplaneConfig,
    discovery: &'cfg ConfigDiscovery,
    context: &'ctx mut CommandContext<'ctx, A, O, S>,
) -> Result<()>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    CommandExecutor::new(config, discovery, context).execute(cli)
}

pub(super) struct CommandExecutor<'cfg, 'ctx, A, O, S> {
    config: &'cfg ThreadplaneConfig,
    pub(super) context: &'ctx mut CommandContext<'ctx, A, O, S>,
    pub(super) discovery: &'cfg ConfigDiscovery,
}

impl<'cfg, 'ctx, A, O, S> CommandExecutor<'cfg, 'ctx, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    const fn new(
        config: &'cfg ThreadplaneConfig,
        discovery: &'cfg ConfigDiscovery,
        context: &'ctx mut CommandContext<'ctx, A, O, S>,
    ) -> Self {
        Self {
            config,
            context,
            discovery,
        }
    }

    fn execute(&mut self, cli: Cli) -> Result<()> {
        let Cli {
            command: root_command,
            idempotency_key: command_idempotency_key,
            ..
        } = cli;
        let idempotency_key = command_idempotency_key.as_deref();

        match root_command {
            Command::Build(build_command) => self.handle_build(&build_command)?,
            Command::Config(config_command) => self.handle_config(&config_command)?,
            Command::Entity(entity_command) => self.handle_entity(entity_command)?,
            Command::Epic(epic_command) => self.handle_epic(idempotency_key, epic_command)?,
            Command::Events(events_command) => self.handle_events(events_command)?,
            Command::Link(link_command) => self.handle_link(idempotency_key, link_command)?,
            Command::Memory(memory_command) => {
                self.handle_memory(idempotency_key, memory_command)?;
            }
            Command::Note(note_command) => self.handle_note(idempotency_key, note_command)?,
            Command::Projection(projection_command) => {
                self.handle_projection(&projection_command)?;
            }
            Command::Scope => self.handle_scope()?,
            Command::Task(task_command) => self.handle_task(idempotency_key, task_command)?,
            Command::Workspace(workspace_command) => {
                self.handle_workspace(idempotency_key, workspace_command)?;
            }
        }

        Ok(())
    }

    fn handle_build(&mut self, command: &BuildCommand) -> Result<()> {
        match command.command {
            BuildSubcommand::Show => self.context.print_value(&current_build_info()),
            BuildSubcommand::Compare => {
                let snapshot: ServiceSnapshot = self.context.get_json("/")?;
                let comparison = compare_build_info(&current_build_info(), &snapshot.build);
                self.context.print_value(&comparison)
            }
        }
    }

    fn handle_config(&mut self, command: &ConfigCommand) -> Result<()> {
        match command.command {
            ConfigSubcommand::Show => {
                let payload = json!({
                    "config": self.config,
                    "discovery": {
                        "search_order": self.discovery.search_order,
                        "selected_path": self.discovery.selected_path,
                        "explicit_override": self.discovery.explicit_override,
                        "env_override": self.discovery.env_override,
                        "env_prefix": self.discovery.env_prefix,
                    }
                });
                self.context.print_value(&payload)
            }
        }
    }

    fn handle_list_tasks(&mut self, task: &ListTasks) -> Result<()> {
        let path = task_list_path(task)?;
        let response: ApiEnvelope<Vec<TaskListEntry>> = self.context.get_json(&path)?;

        match task.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_task_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_next_task(&mut self, task: &NextTask) -> Result<()> {
        let path = task_next_path(task)?;
        let response: ApiEnvelope<Option<TaskListEntry>> = self.context.get_json(&path)?;

        match task.format {
            OutputFormat::Compact => {
                let rendered = response.data.map_or_else(
                    || "no tasks\n".to_owned(),
                    |entry| render_task_list_compact(&[entry]),
                );
                self.context.print_compact(&rendered);
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    fn handle_offer_task(&mut self, idempotency_key: Option<&str>, task: OfferTask) -> Result<()> {
        let workspace_policy = self.fetch_workspace_policy_summary(&task.workspace)?;
        let request = OfferTaskRequest {
            workspace: task.workspace,
            author: task.author,
            depends_on: task.depends_on,
            title: task.title,
            details: task.details,
            epic_id: task.epic_id,
            metadata: task_metadata_from_args(task.metadata, &workspace_policy)?,
        };
        let response: serde_json::Value =
            self.context
                .post_json("/v1/tasks", &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_release_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: ReleaseTask,
    ) -> Result<()> {
        let request = ReleaseTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
        };
        let path = task_claim_release_path(task.task_id);
        let response: serde_json::Value =
            self.context.post_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_show_task(&mut self, task: &ShowTask) -> Result<()> {
        let path = task_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    fn handle_task(&mut self, idempotency_key: Option<&str>, command: TaskCommand) -> Result<()> {
        match command.command {
            TaskSubcommand::BlockedBy(task) => {
                self.handle_task_dependency_view(&task, TaskDependencyViewKind::BlockedBy)
            }
            TaskSubcommand::Blocks(task) => {
                self.handle_task_dependency_view(&task, TaskDependencyViewKind::Blocks)
            }
            TaskSubcommand::ClaimNext(task) => self.handle_claim_next_task(idempotency_key, task),
            TaskSubcommand::Claim(task) => self.handle_claim_task(idempotency_key, task),
            TaskSubcommand::Complete(task) => self.handle_complete_task(idempotency_key, task),
            TaskSubcommand::Context(task) => self.handle_task_context(&task),
            TaskSubcommand::Dag(task) => self.handle_task_dag(&task),
            TaskSubcommand::Depend(task) => self.handle_add_task_dependency(idempotency_key, task),
            TaskSubcommand::List(task) => self.handle_list_tasks(&task),
            TaskSubcommand::Next(task) => self.handle_next_task(&task),
            TaskSubcommand::Offer(task) => self.handle_offer_task(idempotency_key, task),
            TaskSubcommand::Release(task) => self.handle_release_task(idempotency_key, task),
            TaskSubcommand::Show(task) => self.handle_show_task(&task),
            TaskSubcommand::Triage(task) => {
                let response = self.triage_tasks(idempotency_key, &task)?;
                self.context.print_value(&response)
            }
            TaskSubcommand::Update(task) => self.handle_update_task(idempotency_key, task),
        }
    }

    fn handle_task_context(&mut self, task: &TaskContextCommand) -> Result<()> {
        let path = task_context_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    fn handle_task_dag(&mut self, task: &TaskDagCommand) -> Result<()> {
        let path = task_dag_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    fn handle_task_dependency_view(
        &mut self,
        task: &TaskDependencyViewCommand,
        kind: TaskDependencyViewKind,
    ) -> Result<()> {
        let data = if task.direct_only {
            let context = self.fetch_task_context(task.task_id)?;
            select_dependency_view_from_context(&context, kind).to_vec()
        } else {
            let dag = self.fetch_task_dag(task.task_id)?;
            select_dependency_view_from_dag(&dag, kind).to_vec()
        };

        match task.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_task_dependency_compact(&data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&data),
        }
    }

    fn handle_update_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: UpdateTask,
    ) -> Result<()> {
        let workspace_policy = self.fetch_workspace_policy_summary(&task.workspace)?;
        let request = UpdateTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
            title: task.title,
            details: task.details,
            epic_id: task.epic_id,
            metadata: task_metadata_from_args(task.metadata, &workspace_policy)?,
        };
        let path = task_path(task.task_id);
        let response: serde_json::Value =
            self.context.patch_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn fetch_task_context(&self, task_id: Uuid) -> Result<TaskContext> {
        let path = task_context_path(task_id);
        let response: ApiEnvelope<TaskContext> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    fn fetch_task_dag(&self, task_id: Uuid) -> Result<TaskDag> {
        let path = task_dag_path(task_id);
        let response: ApiEnvelope<TaskDag> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    fn fetch_task_summary(&self, task_id: Uuid) -> Result<TaskRecord> {
        let path = task_path(task_id);
        let response: ApiEnvelope<TaskRecord> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    fn handle_add_task_dependency(
        &mut self,
        idempotency_key: Option<&str>,
        task: AddTaskDependency,
    ) -> Result<()> {
        let request = AddTaskDependencyRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
            depends_on_task_id: task.depends_on,
        };
        let response: serde_json::Value = self.context.post_json(
            &task_dependencies_path(task.task_id),
            &request,
            idempotency_key,
        )?;
        self.context.print_value(&response)
    }

    fn handle_claim_next_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: ClaimNextTask,
    ) -> Result<()> {
        let request = ClaimNextTaskRequest {
            actor: task.actor,
            epic_id: task.epic_id,
            label: task
                .label
                .and_then(|value| normalize_task_labels(vec![value]).into_iter().next()),
            lease_seconds: task.lease_seconds,
            owner: normalize_task_owner(task.metadata_filters.owner),
            priority: task
                .metadata_filters
                .priority
                .as_deref()
                .map(parse_task_priority_input)
                .transpose()?,
            workspace: task.workspace,
        };
        let response: ApiEnvelope<Option<TaskClaimRecord>> =
            self.context
                .post_json("/v1/tasks/claims/next", &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_claim_task(&mut self, idempotency_key: Option<&str>, task: ClaimTask) -> Result<()> {
        let request = ClaimTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
            lease_seconds: task.lease_seconds,
        };
        let path = task_claims_path(task.task_id);
        let response: serde_json::Value =
            self.context.post_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn handle_complete_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: CompleteTask,
    ) -> Result<()> {
        let request = CompleteTaskRequest {
            workspace: task.workspace,
            actor: task.actor,
            task_id: task.task_id,
        };
        let path = task_completion_path(task.task_id);
        let response: serde_json::Value =
            self.context.post_json(&path, &request, idempotency_key)?;
        self.context.print_value(&response)
    }

    fn triage_task_record(
        &self,
        idempotency_key: Option<&str>,
        task: &TriageTasks,
        task_id: Uuid,
        task_record: &TaskRecord,
        next_metadata: &TaskMetadata,
    ) -> Result<TaskTriageOutcome> {
        let mut outcome = TaskTriageOutcome::default();

        if let Some(epic_id) = task.epic_id {
            if task_record.epic_id != Some(epic_id) {
                let request = UpdateTaskRequest {
                    workspace: task.workspace.clone(),
                    actor: task.actor.clone(),
                    task_id,
                    title: task_record.title.clone(),
                    details: task_record.details.clone(),
                    epic_id: Some(epic_id),
                    metadata: next_metadata.clone(),
                };
                let request_key = idempotency_key
                    .map(|root_key| format!("{root_key}:triage-update-epic:{task_id}"));
                let _: serde_json::Value = self.context.patch_json(
                    &task_path(task_id),
                    &request,
                    request_key.as_deref(),
                )?;
                outcome.changed = true;
                outcome.updated = true;
            }
        }

        if !outcome.changed && task_metadata_changed(&task_record.metadata, next_metadata) {
            let request = UpdateTaskRequest {
                workspace: task.workspace.clone(),
                actor: task.actor.clone(),
                task_id,
                title: task_record.title.clone(),
                details: task_record.details.clone(),
                epic_id: task_record.epic_id,
                metadata: next_metadata.clone(),
            };
            let request_key =
                idempotency_key.map(|root_key| format!("{root_key}:triage-update-meta:{task_id}"));
            let _: serde_json::Value =
                self.context
                    .patch_json(&task_path(task_id), &request, request_key.as_deref())?;
            outcome.changed = true;
            outcome.updated = true;
        }

        if task.complete && task_record.status != "completed" {
            let request = CompleteTaskRequest {
                workspace: task.workspace.clone(),
                actor: task.actor.clone(),
                task_id,
            };
            let request_key =
                idempotency_key.map(|root_key| format!("{root_key}:triage-complete:{task_id}"));
            let _: serde_json::Value = self.context.post_json(
                &task_completion_path(task_id),
                &request,
                request_key.as_deref(),
            )?;
            outcome.changed = true;
            outcome.completed = true;
        }

        Ok(outcome)
    }

    fn triage_tasks(
        &self,
        idempotency_key: Option<&str>,
        task: &TriageTasks,
    ) -> Result<TaskTriageSummary> {
        if !triage_has_changes(task.complete, task.epic_id, &task.metadata) {
            return Usage {
                message:
                    "task triage needs at least --epic-id, --complete, --priority, --owner, --clear-owner, --label, or --clear-labels"
                        .to_owned(),
            }
            .fail();
        }

        let task_ids = dedup_task_ids(&task.task_id);
        let mut completed_task_ids = Vec::new();
        let mut unchanged_task_ids = Vec::new();
        let mut updated_task_ids = Vec::new();

        for task_id in &task_ids {
            let task_record = self.fetch_task_summary(*task_id)?;
            let next_metadata = apply_metadata_patch(&task_record.metadata, &task.metadata)?;
            let outcome = self.triage_task_record(
                idempotency_key,
                task,
                *task_id,
                &task_record,
                &next_metadata,
            )?;

            if outcome.updated {
                updated_task_ids.push(*task_id);
            }
            if outcome.completed {
                completed_task_ids.push(*task_id);
            }
            if !outcome.changed {
                unchanged_task_ids.push(*task_id);
            }
        }

        Ok(TaskTriageSummary {
            clear_labels: task.metadata.clear_labels,
            clear_owner: task.metadata.clear_owner,
            completed_task_ids,
            epic_id: task.epic_id,
            labels: triage_summary_labels(&task.metadata),
            owner: triage_summary_owner(&task.metadata),
            priority: task
                .metadata
                .priority
                .as_deref()
                .map(parse_task_priority_input)
                .transpose()?,
            task_ids,
            unchanged_task_ids,
            updated_task_ids,
            workspace: task.workspace.clone(),
        })
    }
}
