use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(crate) fn handle_link(
        &mut self,
        idempotency_key: Option<&str>,
        command: LinkCommand,
    ) -> Result<()> {
        match command.command {
            LinkSubcommand::Add(link) => {
                let request = AddLinkRequest {
                    workspace: link.workspace,
                    actor: link.actor,
                    from: link.from,
                    to: link.to,
                    relation: link.relation,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/links", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            LinkSubcommand::Xanadu(link) => {
                let request = CreateXanaduLinkRequest {
                    workspace: link.workspace,
                    actor: link.actor,
                    from: link.from,
                    to: link.to,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/links/xanadu", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
        }
    }
}
