#![allow(
    clippy::wildcard_imports,
    reason = "Workspace command definitions intentionally build on the command module prelude."
)]

use super::*;

#[derive(Debug, Args)]
#[command(about = "Inspect and manage workspace policy, memberships, and public keys")]
pub(crate) struct WorkspaceCommand {
    #[command(subcommand)]
    pub(crate) command: WorkspaceSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkspaceSubcommand {
    #[command(about = "Add or update an actor public key for a workspace")]
    KeyAdd(WorkspaceKeyAdd),
    #[command(about = "List actor public keys registered for a workspace")]
    KeyList(WorkspaceKeyList),
    #[command(about = "Grant or update a workspace membership")]
    MemberGrant(WorkspaceMemberGrant),
    #[command(about = "List workspace memberships")]
    MemberList(WorkspaceMemberList),
    #[command(about = "Replace the workspace governance policy")]
    PolicySet(WorkspacePolicySet),
    #[command(about = "Show the effective workspace governance policy")]
    PolicyShow(WorkspacePolicyShow),
}

#[derive(Debug, Args)]
pub(crate) struct WorkspacePolicyShow {
    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkspacePolicySet {
    #[arg(long, help = "Admin actor updating the workspace policy")]
    pub(crate) actor: String,

    #[arg(
        long = "allowed-algorithm",
        help = "Allowed public-key algorithm. Repeat for multiple algorithms.",
        required = true
    )]
    pub(crate) allowed_algorithms: Vec<String>,

    #[arg(long, help = "Challenge TTL in seconds")]
    pub(crate) challenge_ttl_seconds: u32,

    #[arg(long, help = "Default task priority name")]
    pub(crate) default_priority: String,

    #[arg(
        long = "priority",
        help = "Priority definition as name:rank[:description]. Repeat for multiple priorities.",
        required = true
    )]
    pub(crate) priorities: Vec<String>,

    #[arg(long, help = "Require signed commands for workspace mutations")]
    pub(crate) signed_commands_required: bool,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkspaceMemberList {
    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkspaceMemberGrant {
    #[arg(long, help = "Admin actor granting the membership")]
    pub(crate) actor: String,

    #[arg(long, help = "Member actor ID")]
    pub(crate) member_actor_id: String,

    #[arg(long, help = "Workspace role to grant")]
    pub(crate) role: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkspaceKeyList {
    #[arg(long, help = "Optional actor ID filter")]
    pub(crate) actor_id: Option<String>,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkspaceKeyAdd {
    #[arg(long, help = "Admin actor registering the key")]
    pub(crate) actor: String,

    #[arg(long, help = "Public-key algorithm")]
    pub(crate) algorithm: String,

    #[arg(long, help = "Key ID")]
    pub(crate) key_id: String,

    #[arg(long, help = "Member actor ID")]
    pub(crate) member_actor_id: String,

    #[arg(long, help = "Public key material")]
    pub(crate) public_key: String,

    #[arg(long, help = "Workspace name")]
    pub(crate) workspace: String,
}
