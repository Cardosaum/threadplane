# Workspace Governance and Pubkey Auth

This document captures the next major product slice after the current POC: workspace-governed task priorities, explicit membership roles, and public-key authentication.

## Why

Today, `threadplane` still assumes:

- one global hardcoded priority vocabulary: `low`, `medium`, `high`, `urgent`
- implicit actor trust at the API boundary
- no first-class distinction between workspace viewers, editors, and admins

That keeps the POC simple, but it is not a good long-term shape for real shared workspaces.

The target direction is:

- each workspace can define its own supported priority vocabulary and ordering
- each workspace can assign role-based capabilities to members
- every mutating request is authenticated with public keys instead of shared secrets

## Product Model

### Workspace Priorities

Priorities should become workspace policy, not a global enum baked into the binary.

Each workspace should eventually define:

- a set of supported priorities
- a stable ordering rank for scheduling and ready-queue sorting
- one default priority used when task creation omits an explicit priority
- optional human-facing descriptions

Example:

```toml
[workspace_policy.priorities]
default_priority = "normal"

[[workspace_policy.priorities.priorities]]
name = "background"
rank = 10
description = "Useful but not urgent."

[[workspace_policy.priorities.priorities]]
name = "normal"
rank = 20
description = "Expected day-to-day work."

[[workspace_policy.priorities.priorities]]
name = "expedite"
rank = 30
description = "Pull forward ahead of normal backlog."
```

Important rule:

- the server sorts by `rank`, not by hardcoded string meaning

### Workspace Roles

`threadplane` should expose three explicit roles:

- `viewer`
- `editor`
- `admin`

Capability matrix:

- `viewer`
  - can inspect workspace state
  - can list tasks, notes, events, entity context, and projections
  - cannot mutate notes, tasks, links, workspace policy, or membership
- `editor`
  - can do everything a viewer can do
  - can create and update notes, tasks, links, claims, releases, and completions
  - cannot change workspace policy or membership
- `admin`
  - can do everything an editor can do
  - can manage workspace policy
  - can manage membership and role assignment
  - can manage trusted public keys

### Public-Key Auth

Authentication should be public-key based, closer to Git or blockchain wallet signing than bearer-token sharing.

The intended shape:

1. client asks for a short-lived challenge or server-provided nonce
2. client signs a canonical representation of the request using a registered private key
3. server verifies the signature against a trusted public key registered for the actor
4. server resolves actor membership and role for the target workspace
5. server authorizes the request against workspace policy

Why this is preferable here:

- no long-lived shared secret to leak from an agent runtime
- better fit for multi-agent and multi-machine usage
- key registration and revocation are explicit workspace governance actions
- command provenance becomes stronger because signatures bind actor identity to a concrete request

## Implementation Phases

### Phase 1: Shared Domain Model

Land first-class shared types and validation for:

- `WorkspaceRole`
- `WorkspacePriority`
- `WorkspacePriorityPolicy`
- `WorkspaceAuthPolicy`
- `WorkspaceMembership`
- `ActorPublicKey`

This phase is about clean abstractions, not about turning on auth yet.

### Phase 2: Workspace Policy Storage

Add PostgreSQL tables for:

- `workspace_policies`
- `workspace_priorities`
- `workspace_memberships`
- `actor_public_keys`

The server should read policy from PostgreSQL, not from a single global TOML file, because workspace policy is shared durable state.

### Phase 3: Dynamic Priority Adoption

Replace hardcoded task-priority assumptions with workspace-ranked priorities.

That includes:

- task writes validating priority names against workspace policy
- ready queues sorting by workspace rank
- CLI reads rendering the workspace-defined priority string
- list/query filters using the workspace-defined priority value

### Phase 4: Signed Request Flow

Add the first public-key auth path:

- register public keys for actors
- challenge/nonce endpoint
- canonical request signing format
- signature verification middleware or extractor
- role checks in mutation handlers

### Phase 5: Admin Surface

Expose first-class admin APIs and CLI commands for:

- workspace policy reads and updates
- membership role management
- public-key registration and revocation

## Design Constraints

- workspace policy is durable shared state, not only local config
- server-side validation is authoritative
- CLI-side validation is only a convenience layer
- role checks should be centralized, not hand-coded in each handler
- priority sorting must use workspace rank, not lexical order
- signed requests should bind method, path, body digest, nonce, actor, and timestamp

## Immediate Follow-Up

The current foundational step is:

- keep the POC working as-is
- add shared governance/auth types and validation helpers now
- use those types to drive the next PostgreSQL and HTTP changes
