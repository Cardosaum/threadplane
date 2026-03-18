use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(super) fn handle_entity(&mut self, command: EntityCommand) -> Result<()> {
        match command.command {
            EntitySubcommand::Show(entity) => self.handle_show_entity(&entity),
            EntitySubcommand::Related(entity) => self.handle_related_entities(&entity),
        }
    }

    pub(super) fn handle_events(&mut self, command: EventsCommand) -> Result<()> {
        match command.command {
            EventsSubcommand::List(events) => {
                let path = events_list_path(events.workspace.as_str(), events.limit);
                let response: ApiEnvelope<Vec<EventRecord>> = self.context.get_json(&path)?;
                match events.format {
                    OutputFormat::Compact => {
                        self.context
                            .print_compact(&render_event_list_compact(&response.data));
                        Ok(())
                    }
                    OutputFormat::Json => self.context.print_value(&response),
                }
            }
            EventsSubcommand::Tail(events) => self.handle_tail_events(&events),
        }
    }

    pub(super) fn handle_projection(&mut self, command: &ProjectionCommand) -> Result<()> {
        match command.command {
            ProjectionSubcommand::Status => {
                let response: ApiEnvelope<ProjectionStatus> =
                    self.context.get_json("/v1/projections/graph")?;
                self.context.print_value(&response)
            }
        }
    }

    pub(super) fn handle_related_entities(&mut self, entity: &RelatedEntities) -> Result<()> {
        let path = entity_relations_path(entity.entity_ref.as_str());
        let response: ApiEnvelope<Vec<GraphRelation>> = self.context.get_json(&path)?;

        match entity.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_graph_relations_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    pub(super) fn handle_scope(&mut self) -> Result<()> {
        let scope: serde_json::Value = self.context.get_json("/scope")?;
        let snapshot: ServiceSnapshot = self.context.get_json("/")?;
        let comparison = compare_build_info(&current_build_info(), &snapshot.build);

        if let Some(warning) = build_mismatch_warning(&comparison) {
            self.context.warn(&format!("warning: {warning}"));
        }

        self.context.print_value(&scope)
    }

    pub(super) fn handle_show_entity(&mut self, entity: &ShowEntity) -> Result<()> {
        let path = entity_show_path(entity.entity_ref.as_str());
        let response: ApiEnvelope<EntityContext> = self.context.get_json(&path)?;

        match entity.format {
            OutputFormat::Compact => {
                self.context
                    .print_compact(&render_entity_context_compact(&response.data));
                Ok(())
            }
            OutputFormat::Json => self.context.print_value(&response),
        }
    }

    pub(super) fn handle_tail_events(&mut self, events: &TailEvents) -> Result<()> {
        let mut cursor = events.after_event_id;

        loop {
            let path = events_tail_path(events.workspace.as_str(), events.limit, cursor);
            let response: ApiEnvelope<Vec<EventRecord>> = self.context.get_json(&path)?;
            let latest_event_id = response.data.last().map(|event| event.event_id);

            match events.format {
                OutputFormat::Compact => {
                    if !response.data.is_empty() {
                        self.context
                            .print_compact(&render_event_list_compact(&response.data));
                    }
                    if response.data.is_empty() && !events.follow {
                        self.context.print_compact("no events\n");
                    }
                }
                OutputFormat::Json => self.context.print_value(&response)?,
            }

            cursor = latest_event_id.or(cursor);
            if !events.follow {
                return Ok(());
            }

            self.context.sleep(Duration::from_secs(events.poll_seconds));
        }
    }
}
