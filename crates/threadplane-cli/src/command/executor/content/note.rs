use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(crate) fn handle_note(
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

    pub(crate) fn handle_list_notes(&mut self, note: &ListNotes) -> Result<()> {
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

    pub(crate) fn handle_search_notes(&mut self, note: &SearchNotes) -> Result<()> {
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
