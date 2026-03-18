use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(super) fn handle_add_task_dependency(
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

    pub(super) fn handle_offer_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: OfferTask,
    ) -> Result<()> {
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

    pub(super) fn handle_update_task(
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

    pub(super) fn triage_task_record(
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

    pub(super) fn triage_tasks(
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
