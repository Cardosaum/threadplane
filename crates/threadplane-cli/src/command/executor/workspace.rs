use super::*;

impl<A, O, S> CommandExecutor<'_, '_, A, O, S>
where
    A: ApiClient,
    O: CommandOutput,
    S: Sleeper,
{
    pub(super) fn handle_workspace(
        &mut self,
        idempotency_key: Option<&str>,
        command: WorkspaceCommand,
    ) -> Result<()> {
        match command.command {
            WorkspaceSubcommand::PolicyShow(workspace) => {
                let response: ApiEnvelope<WorkspacePolicy> = self
                    .context
                    .get_json(&workspace_policy_path(&workspace.workspace))?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::PolicySet(workspace) => {
                let request = UpdateWorkspacePolicyRequest {
                    actor: workspace.actor,
                    auth: WorkspaceAuthPolicy {
                        allowed_algorithms: parse_public_key_algorithms(
                            &workspace.allowed_algorithms,
                        )?,
                        challenge_ttl_seconds: workspace.challenge_ttl_seconds,
                        signed_commands_required: workspace.signed_commands_required,
                    },
                    priorities: WorkspacePriorityPolicy {
                        default_priority: normalize_priority_name(&workspace.default_priority)?,
                        priorities: parse_workspace_priority_specs(&workspace.priorities)?,
                    },
                    workspace: workspace.workspace.clone(),
                };
                let response: ApiEnvelope<WorkspacePolicy> = self.context.put_json(
                    &workspace_policy_path(&workspace.workspace),
                    &request,
                    idempotency_key,
                )?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::MemberList(workspace) => {
                let response: ApiEnvelope<Vec<WorkspaceMembership>> = self
                    .context
                    .get_json(&workspace_memberships_path(&workspace.workspace))?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::MemberGrant(workspace) => {
                let request = GrantWorkspaceMembershipRequest {
                    actor: workspace.actor,
                    member_actor_id: workspace.member_actor_id,
                    role: parse_workspace_role(&workspace.role)?,
                    workspace: workspace.workspace.clone(),
                };
                let response: ApiEnvelope<WorkspaceMembership> = self.context.post_json(
                    &workspace_memberships_path(&workspace.workspace),
                    &request,
                    idempotency_key,
                )?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::KeyList(workspace) => {
                let response: ApiEnvelope<Vec<ActorPublicKey>> = self.context.get_json(
                    &workspace_keys_path(&workspace.workspace, workspace.actor_id.as_deref()),
                )?;
                self.context.print_value(&response)
            }
            WorkspaceSubcommand::KeyAdd(workspace) => {
                let request = AddWorkspacePublicKeyRequest {
                    actor: workspace.actor,
                    algorithm: parse_public_key_algorithm(&workspace.algorithm)?,
                    key_id: workspace.key_id,
                    member_actor_id: workspace.member_actor_id,
                    public_key: workspace.public_key,
                    workspace: workspace.workspace.clone(),
                };
                let response: ApiEnvelope<ActorPublicKey> = self.context.post_json(
                    &workspace_keys_path(&workspace.workspace, None),
                    &request,
                    idempotency_key,
                )?;
                self.context.print_value(&response)
            }
        }
    }

    pub(super) fn fetch_workspace_policy_summary(
        &self,
        workspace: &str,
    ) -> Result<WorkspacePolicy> {
        let response: ApiEnvelope<WorkspacePolicy> =
            self.context.get_json(&workspace_policy_path(workspace))?;
        Ok(response.data)
    }
}
