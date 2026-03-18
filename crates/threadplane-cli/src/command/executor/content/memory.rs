use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(crate) fn handle_memory(
        &mut self,
        idempotency_key: Option<&str>,
        command: MemoryCommand,
    ) -> Result<()> {
        match command.command {
            MemorySubcommand::Add(add) => {
                let request = CreateMemoryRequest {
                    workspace: add.workspace,
                    author: add.author,
                    title: add.title,
                    body: add.body,
                    kind: parse_memory_kind_input(&add.kind)?,
                    scope: parse_memory_scope_input(&add.scope)?,
                    audience: parse_memory_audience_input(&add.audience)?,
                    importance: parse_memory_importance_input(&add.importance)?,
                    tags: add.tags,
                    recall_triggers: add.recall_triggers,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/memories", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            MemorySubcommand::List(list) => self.handle_list_memories(&list),
            MemorySubcommand::Prime(prime) => self.handle_prime_memories(&prime),
            MemorySubcommand::Show(show) => {
                let path = memory_path(show.memory_id);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
        }
    }

    pub(crate) fn handle_list_memories(&mut self, memory: &ListMemories) -> Result<()> {
        let path = memory_list_path(MemoryListPathArgs {
            audience: memory.audience.as_deref(),
            importance: memory.importance.as_deref(),
            kind: memory.kind.as_deref(),
            limit: memory.limit,
            query: memory.query.as_deref(),
            recall_trigger: memory.recall_trigger.as_deref(),
            tag: memory.tag.as_deref(),
            workspace: memory.workspace.as_str(),
        })?;
        let response: ApiEnvelope<Vec<MemoryRecord>> = self.context.get_json(&path)?;

        match memory.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_memory_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    pub(crate) fn handle_prime_memories(&mut self, memory: &PrimeMemories) -> Result<()> {
        let path = memory_prime_path(memory)?;
        let response: ApiEnvelope<Vec<MemoryRecord>> = self.context.get_json(&path)?;

        match memory.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_memory_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }
}
