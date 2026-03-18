use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(crate) fn handle_epic(
        &mut self,
        idempotency_key: Option<&str>,
        command: EpicCommand,
    ) -> Result<()> {
        match command.command {
            EpicSubcommand::Add(epic) => {
                let request = CreateEpicRequest {
                    workspace: epic.workspace,
                    author: epic.author,
                    title: epic.title,
                    body: epic.body,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/epics", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            EpicSubcommand::List(epics) => {
                let path = format!("/v1/workspaces/{}/epics", epics.workspace);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
            EpicSubcommand::Show(epic) => {
                let path = format!("/v1/epics/{}", epic.epic_id);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
        }
    }
}
