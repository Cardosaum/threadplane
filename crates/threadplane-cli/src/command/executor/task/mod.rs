use super::*;

mod claims;
mod mutations;
mod reads;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(super) fn handle_task(
        &mut self,
        idempotency_key: Option<&str>,
        command: TaskCommand,
    ) -> Result<()> {
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
}
