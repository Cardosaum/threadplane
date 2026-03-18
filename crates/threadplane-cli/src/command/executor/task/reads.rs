use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(super) fn fetch_task_context(&self, task_id: Uuid) -> Result<TaskContext> {
        let path = task_context_path(task_id);
        let response: ApiEnvelope<TaskContext> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    pub(super) fn fetch_task_dag(&self, task_id: Uuid) -> Result<TaskDag> {
        let path = task_dag_path(task_id);
        let response: ApiEnvelope<TaskDag> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    pub(super) fn fetch_task_summary(&self, task_id: Uuid) -> Result<TaskRecord> {
        let path = task_path(task_id);
        let response: ApiEnvelope<TaskRecord> = self.context.get_json(&path)?;
        Ok(response.data)
    }

    pub(super) fn handle_list_tasks(&mut self, task: &ListTasks) -> Result<()> {
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

    pub(super) fn handle_next_task(&mut self, task: &NextTask) -> Result<()> {
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

    pub(super) fn handle_show_task(&mut self, task: &ShowTask) -> Result<()> {
        let path = task_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    pub(super) fn handle_task_context(&mut self, task: &TaskContextCommand) -> Result<()> {
        let path = task_context_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    pub(super) fn handle_task_dag(&mut self, task: &TaskDagCommand) -> Result<()> {
        let path = task_dag_path(task.task_id);
        let response: serde_json::Value = self.context.get_json(&path)?;
        self.context.print_value(&response)
    }

    pub(super) fn handle_task_dependency_view(
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
}
