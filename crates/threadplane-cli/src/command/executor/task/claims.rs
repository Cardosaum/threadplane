use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(super) fn handle_claim_next_task(
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

    pub(super) fn handle_claim_task(
        &mut self,
        idempotency_key: Option<&str>,
        task: ClaimTask,
    ) -> Result<()> {
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

    pub(super) fn handle_complete_task(
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

    pub(super) fn handle_release_task(
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
}
