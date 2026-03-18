use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(super) fn handle_epic(
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

    pub(super) fn handle_link(
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

    pub(super) fn handle_list_memories(&mut self, memory: &ListMemories) -> Result<()> {
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

    pub(super) fn handle_list_notes(&mut self, note: &ListNotes) -> Result<()> {
        let path = note_list_path(
            note.workspace.as_str(),
            note.limit,
            note.author.as_deref(),
            None,
        );
        let response: ApiEnvelope<Vec<NoteRecord>> = self.context.get_json(&path)?;

        match note.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_note_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    pub(super) fn handle_memory(
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

    pub(super) fn handle_note(
        &mut self,
        idempotency_key: Option<&str>,
        command: NoteCommand,
    ) -> Result<()> {
        match command.command {
            NoteSubcommand::Add(add) => {
                let request = CreateNoteRequest {
                    workspace: add.workspace,
                    author: add.author,
                    title: add.title,
                    body: add.body,
                };
                let response: serde_json::Value =
                    self.context
                        .post_json("/v1/notes", &request, idempotency_key)?;
                self.context.print_value(&response)
            }
            NoteSubcommand::List(list) => self.handle_list_notes(&list),
            NoteSubcommand::Search(search) => self.handle_search_notes(&search),
            NoteSubcommand::Show(show) => {
                let path = note_path(show.note_id);
                let response: serde_json::Value = self.context.get_json(&path)?;
                self.context.print_value(&response)
            }
            NoteSubcommand::Update(update) => {
                let path = note_path(update.note_id);
                let request = UpdateNoteRequest {
                    workspace: update.workspace,
                    actor: update.actor,
                    note_id: update.note_id,
                    title: update.title,
                    body: update.body,
                };
                let response: serde_json::Value =
                    self.context.patch_json(&path, &request, idempotency_key)?;
                self.context.print_value(&response)
            }
        }
    }

    pub(super) fn handle_prime_memories(&mut self, memory: &PrimeMemories) -> Result<()> {
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

    pub(super) fn handle_search_notes(&mut self, note: &SearchNotes) -> Result<()> {
        let path = note_list_path(
            note.workspace.as_str(),
            note.limit,
            note.author.as_deref(),
            Some(note.query.as_str()),
        );
        let response: ApiEnvelope<Vec<NoteRecord>> = self.context.get_json(&path)?;

        match note.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_note_list_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }
}
