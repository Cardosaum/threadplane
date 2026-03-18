#![expect(
    clippy::arbitrary_source_item_ordering,
    clippy::redundant_pub_crate,
    reason = "Executor keeps command flow grouped by surface area and uses command-module boundaries as its import seam."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "Executor uses the command module as its import boundary to keep the orchestration layer concise."
)]

use super::paths::*;
use super::render::*;
use super::*;

mod content;
mod reads;
mod task;
mod workspace;

pub(crate) fn execute<'cfg, 'ctx, A, O, S>(
    cli: Cli,
    config: &'cfg ThreadplaneConfig,
    discovery: &'cfg ConfigDiscovery,
    context: &'ctx mut CommandContext<'ctx, A, O, S>,
) -> Result<()>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    CommandExecutor::new(config, discovery, context).execute(cli)
}

pub(super) struct CommandExecutor<'cfg, 'ctx, A, O, S> {
    config: &'cfg ThreadplaneConfig,
    pub(super) context: &'ctx mut CommandContext<'ctx, A, O, S>,
    pub(super) discovery: &'cfg ConfigDiscovery,
}

impl<'cfg, 'ctx, A, O, S> CommandExecutor<'cfg, 'ctx, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    const fn new(
        config: &'cfg ThreadplaneConfig,
        discovery: &'cfg ConfigDiscovery,
        context: &'ctx mut CommandContext<'ctx, A, O, S>,
    ) -> Self {
        Self {
            config,
            context,
            discovery,
        }
    }

    fn execute(&mut self, cli: Cli) -> Result<()> {
        let Cli {
            command: root_command,
            idempotency_key: command_idempotency_key,
            ..
        } = cli;
        let idempotency_key = command_idempotency_key.as_deref();

        match root_command {
            Command::Build(build_command) => self.handle_build(&build_command)?,
            Command::Config(config_command) => self.handle_config(&config_command)?,
            Command::Entity(entity_command) => self.handle_entity(entity_command)?,
            Command::Epic(epic_command) => self.handle_epic(idempotency_key, epic_command)?,
            Command::Events(events_command) => self.handle_events(events_command)?,
            Command::Link(link_command) => self.handle_link(idempotency_key, link_command)?,
            Command::Memory(memory_command) => {
                self.handle_memory(idempotency_key, memory_command)?;
            }
            Command::Note(note_command) => self.handle_note(idempotency_key, note_command)?,
            Command::Projection(projection_command) => {
                self.handle_projection(&projection_command)?;
            }
            Command::Scope => self.handle_scope()?,
            Command::Task(task_command) => self.handle_task(idempotency_key, task_command)?,
            Command::Workspace(workspace_command) => {
                self.handle_workspace(idempotency_key, workspace_command)?;
            }
        }

        Ok(())
    }

    fn handle_build(&mut self, command: &BuildCommand) -> Result<()> {
        match command.command {
            BuildSubcommand::Show => self.context.print_value(&current_build_info()),
            BuildSubcommand::Compare => {
                let snapshot: ServiceSnapshot = self.context.get_json("/")?;
                let comparison = compare_build_info(&current_build_info(), &snapshot.build);
                self.context.print_value(&comparison)
            }
        }
    }

    fn handle_config(&mut self, command: &ConfigCommand) -> Result<()> {
        match command.command {
            ConfigSubcommand::Show => {
                let payload = json!({
                    "config": self.config,
                    "discovery": {
                        "search_order": self.discovery.search_order,
                        "selected_path": self.discovery.selected_path,
                        "explicit_override": self.discovery.explicit_override,
                        "env_override": self.discovery.env_override,
                        "env_prefix": self.discovery.env_prefix,
                    }
                });
                self.context.print_value(&payload)
            }
        }
    }
}
