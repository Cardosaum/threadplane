# threadplane

[![threadplane demo](./docs/threadplane-demo.gif)](./docs/threadplane-demo.cast)

`threadplane` is a shared memory and coordination plane for people and AI agents. One server accepts writes, PostgreSQL keeps the durable source of truth, Neo4j exposes traversable graph context, and the CLI gives humans and agents one place to leave notes, tasks, dependencies, claims, and shared text for each other.

> [!TIP]
> Click the animation above to open the source asciinema cast. Regenerate the demo locally with `./scripts/regenerate-demo.sh`.

> [!IMPORTANT]
> This repository is a working POC, not a finished platform. The current implementation is already useful for dogfooding and local/shared development, but the cryptographic request-signing flow is still the next major layer.

## Table Of Contents

- [Why Use It](#why-use-it)
- [What You Can Do Today](#what-you-can-do-today)
- [Quick Start](#quick-start)
- [Two-Minute Walkthrough](#two-minute-walkthrough)
- [Core Concepts](#core-concepts)
- [Workspace Governance](#workspace-governance)
- [CLI Overview](#cli-overview)
- [Configuration](#configuration)
- [Architecture At A Glance](#architecture-at-a-glance)
- [Repository Layout](#repository-layout)
- [Read Next](#read-next)

## Why Use It

Most agent tooling gets one or two pieces right:

- memory, but not collaboration
- collaboration, but not durable graph-backed context
- graph context, but not first-class task handoff and dependency tracking
- event logs, but not a CLI-first workflow humans and agents can share

`threadplane` is meant to close that gap with one control plane:

- shared workspaces for people and AI agents
- durable append-only history in PostgreSQL
- graph-backed traversal in Neo4j
- first-class epics and task DAGs
- lease-backed claims for safe handoff
- Xanadu-style transclusion links between notes and tasks
- CLI and HTTP surfaces over the same model

## What You Can Do Today

- create epics, tasks, notes, and graph links
- model task dependencies as a DAG
- list only ready work whose prerequisites are done
- claim, release, and complete tasks with expiring leases
- attach durable metadata like owner, labels, and priority
- create Xanadu links so note and task text stay synchronized
- inspect graph-backed context from tasks, notes, and epics
- tail workspace events incrementally
- configure workspace policy for priorities, memberships, and public keys

## Quick Start

### 1. Generate local config and credentials

```bash
$ ./scripts/generate-env.sh
generated /home/you/path/to/threadplane/.env
generated /home/you/.config/threadplane/config.toml
```

This creates:

- `.env` for Docker Compose
- `${XDG_CONFIG_HOME:-$HOME/.config}/threadplane/config.toml` for `threadplane-server` and `tplane`

### 2. Start PostgreSQL and Neo4j

```bash
$ docker compose up -d
[+] Running 2/2
 ✔ Container threadplane-postgres-1  Started
 ✔ Container threadplane-neo4j-1     Started
```

### 3. Start the API server

```bash
$ cargo run -p threadplane-server
threadplane_server: bootstrapping server runtime
threadplane_server: server listening
```

On startup the server:

- applies versioned SQL migrations
- bootstraps workspace policy on first use
- catches the Neo4j projection up from PostgreSQL
- keeps a replay worker running with durable projection offsets

### 4. Install the CLI

```bash
$ cargo install --path crates/threadplane-cli --locked
  Installing tplane v0.1.0 (...)
   Installed package `tplane`
```

If you do not want to install it yet, use `./target/debug/tplane` after `cargo build -p threadplane-cli`.

### 5. Sanity-check the service

```bash
$ tplane scope
{
  "build": {
    "service": "threadplane-server",
    "version": "0.1.0"
  },
  "projection": {
    "projection_name": "neo4j_graph",
    "caught_up": true
  }
}

$ tplane projection status
{
  "ok": true,
  "data": {
    "projection_name": "neo4j_graph",
    "caught_up": true,
    "pending_events": 0
  }
}

$ tplane build compare
{
  "matches": true,
  "differences": []
}
```

### 6. Run the full smoke test

```bash
$ ./scripts/e2e.sh
threadplane e2e ok
workspace=e2e-...
```

> [!NOTE]
> For day-to-day local dogfooding, `./scripts/dogfood.sh up` is the fastest path. For repeatable perf checks, use `./scripts/benchmark.sh mixed`.

## Two-Minute Walkthrough

Create an epic:

```bash
$ tplane epic add \
    --workspace shared-lab \
    --author operator \
    --title "Workflow foundations" \
    --body "Shared backlog for the workspace."
{
  "ok": true,
  "data": {
    "epic_id": "<epic-id>",
    "entity_ref": "epic:<epic-id>",
    "title": "Workflow foundations",
    "workspace": "shared-lab"
  }
}
```

Offer a prerequisite task:

```bash
$ tplane task offer \
    --workspace shared-lab \
    --author operator \
    --epic-id <epic-id> \
    --title "Ship durable task lifecycle" \
    --details "Completion should unlock dependent work."
{
  "ok": true,
  "data": {
    "task_id": "<dependency-task-id>",
    "status": "open",
    "priority": "medium"
  }
}
```

Offer a dependent task:

```bash
$ tplane task offer \
    --workspace shared-lab \
    --author operator \
    --epic-id <epic-id> \
    --depends-on <dependency-task-id> \
    --priority high \
    --owner codex \
    --label agent \
    --label workflow \
    --title "Investigate tuple leases" \
    --details "Need a shared lease-backed claim flow with dependency tracking."
{
  "ok": true,
  "data": {
    "task_id": "<task-id>",
    "status": "open",
    "priority": "high",
    "owner": "codex",
    "labels": ["agent", "workflow"]
  }
}
```

Inspect the dependency shape:

```bash
$ tplane task dag --task-id <task-id>
{
  "ok": true,
  "data": {
    "ready": false,
    "dependencies": [
      {
        "task_id": "<dependency-task-id>",
        "title": "Ship durable task lifecycle",
        "depth": 1
      }
    ]
  }
}
```

Complete the prerequisite, then ask for ready work:

```bash
$ tplane task complete \
    --workspace shared-lab \
    --actor operator \
    --task-id <dependency-task-id>
{
  "ok": true,
  "data": {
    "task_id": "<dependency-task-id>",
    "status": "completed"
  }
}

$ tplane task list \
    --workspace shared-lab \
    --status open \
    --ready-only \
    --limit 5 \
    --format compact
11ef6dff | Investigate tuple leases | status=open | priority=high | ready | deps=1 | owner=codex | labels=agent,workflow
```

Claim the best next task directly:

```bash
$ tplane task claim-next \
    --workspace shared-lab \
    --actor agent-b \
    --lease-seconds 120
{
  "ok": true,
  "data": {
    "task_id": "<task-id>",
    "claim_id": "<claim-id>",
    "actor": "agent-b",
    "expires_at": "<timestamp>"
  }
}
```

Create a note and link it with Xanadu semantics:

```bash
$ tplane note add \
    --workspace shared-lab \
    --author agent-a \
    --title "Lease design note" \
    --body "Claims should expire and return tasks to the pool."
{
  "ok": true,
  "data": {
    "note_id": "<note-id>",
    "entity_ref": "note:<note-id>"
  }
}

$ tplane link xanadu \
    --workspace shared-lab \
    --actor agent-a \
    --from task:<task-id> \
    --to note:<note-id>
{
  "ok": true,
  "data": {
    "relation": "xanadu_link",
    "transclusion_id": "<transclusion-id>"
  }
}
```

Update one side and inspect the shared context:

```bash
$ tplane note update \
    --workspace shared-lab \
    --actor agent-a \
    --note-id <note-id> \
    --title "Lease semantics updated" \
    --body "A xanadu link should keep linked task text synchronized."
{
  "ok": true,
  "data": {
    "note_id": "<note-id>",
    "title": "Lease semantics updated",
    "transclusion_id": "<transclusion-id>"
  }
}

$ tplane task context --task-id <task-id>
{
  "ok": true,
  "data": {
    "task": {
      "task_id": "<task-id>",
      "transclusion_id": "<transclusion-id>"
    },
    "relations": [
      {
        "relation": "XANADU_LINK"
      }
    ]
  }
}
```

Tail the workspace event history:

```bash
$ tplane events tail --workspace shared-lab --limit 25 --format compact
16766a75 | epic_recorded | actor=operator | at=...
3f85537c | task_offered | actor=operator | at=...
5e9e9fed | task_offered | actor=operator | at=...
16f3f40e | note_recorded | actor=agent-a | at=...
f2548a6a | xanadu_linked | actor=agent-a | at=...
```

## Core Concepts

### Workspace

A shared namespace where agents and people collaborate. Tasks, notes, epics, links, memberships, and policy are all scoped to a workspace.

### Epic

A first-class planning object. Tasks can attach to one epic.

### Task DAG

Tasks can depend on other tasks. Ready queues only surface tasks whose dependencies are already completed.

### Lease-Backed Claim

Agents can claim tasks temporarily. If they disappear, the lease expires and the work returns to the queue.

### Xanadu Link

A special link between text entities that keeps their shared text synchronized through a transclusion group.

### Graph Projection

PostgreSQL is the source of truth. Neo4j is a rebuildable projection used for traversal, relationships, and richer context reads.

## Workspace Governance

Each workspace has a durable governance policy:

- supported task priorities and their ranking
- one default priority used when task creation omits `--priority`
- role membership for `viewer`, `editor`, and `admin`
- allowed public-key algorithms
- registered actor public keys

The server bootstraps new workspaces from `server.workspace_bootstrap` in config the first time they are touched.

Show the effective workspace policy:

```bash
$ tplane workspace policy show --workspace shared-lab
{
  "ok": true,
  "data": {
    "workspace": "shared-lab",
    "priorities": {
      "default_priority": "medium"
    }
  }
}
```

Replace the policy:

```bash
$ tplane workspace policy set \
    --workspace shared-lab \
    --actor operator \
    --default-priority normal \
    --allowed-algorithm ssh_ed25519 \
    --challenge-ttl-seconds 90 \
    --priority background:10:"Useful but not urgent." \
    --priority normal:20:"Default day-to-day work." \
    --priority expedite:30:"Pull this forward." \
    --signed-commands-required
{
  "ok": true,
  "data": {
    "workspace": "shared-lab"
  }
}
```

Grant a workspace member:

```bash
$ tplane workspace member grant \
    --workspace shared-lab \
    --actor operator \
    --member-actor-id agent-c \
    --role editor
{
  "ok": true,
  "data": {
    "actor_id": "agent-c",
    "role": "editor",
    "workspace": "shared-lab"
  }
}
```

> [!IMPORTANT]
> Role-based enforcement is live now, but actor identity is not yet cryptographically verified. Public keys are durable already; challenge issuance and signature verification are the next layer.

## CLI Overview

High-value commands:

- `tplane scope`
- `tplane projection status`
- `tplane build compare`
- `tplane epic add|list|show`
- `tplane task offer|list|next|claim|claim-next|release|complete|show|context|dag|depend|blocked-by|blocks|triage|update`
- `tplane note add|list|search|show|update`
- `tplane entity show|related`
- `tplane workspace policy show|set`
- `tplane workspace member list|grant`
- `tplane workspace key list|add`
- `tplane link add|xanadu`
- `tplane events list|tail`
- `tplane config show`

Global options:

- `--config <path>`
- `--server <url>`
- `--idempotency-key <key>`

## Configuration

`threadplane` does not ship implicit runtime defaults. Every runtime value must come from config, environment, or explicit overrides.

Discovery order:

1. `--config /path/to/config.toml`
2. `THREADPLANE_CONFIG=/path/to/config.toml`
3. `${XDG_CONFIG_HOME:-$HOME/.config}/threadplane/config.toml`
4. XDG system config directories such as `/etc/xdg/threadplane/config.toml`
5. `THREADPLANE__...` nested environment overrides
6. CLI runtime overrides such as `--server`

Inspect the final resolved config:

```bash
$ tplane config show
{
  "config": {
    "cli": {
      "url": "http://127.0.0.1:4000"
    }
  },
  "discovery": {
    "selected_path": "/home/you/.config/threadplane/config.toml"
  }
}
```

For the full config shape, see [etc/config.toml.example](./etc/config.toml.example) and [docs/configuration.md](./docs/configuration.md).

## Architecture At A Glance

- `threadplane-server` is the write boundary
- PostgreSQL is the source of truth
- Neo4j is a rebuildable projection
- projection offsets are stored durably in PostgreSQL
- task coordination semantics live in the service layer
- workspace governance is durable shared state, not just local config

## Repository Layout

- `crates/threadplane-core`: shared types, config, and domain helpers
- `crates/threadplane-server`: HTTP server, storage, and projections
- `crates/threadplane-cli`: CLI for people and agents
- `crates/threadplane-bench`: repeatable benchmark harness
- `compose.yaml`: local PostgreSQL and Neo4j stack
- `docs/`: onboarding, API, architecture, benchmarks, and roadmap docs

## Read Next

- [Onboarding guide](./docs/onboarding.md)
- [Configuration guide](./docs/configuration.md)
- [HTTP API reference](./docs/http-api.md)
- [Benchmarking guide](./docs/benchmarking.md)
- [CLI usability roadmap](./docs/roadmap-cli-usability.md)
- [Governance and auth roadmap](./docs/roadmap-governance-and-auth.md)
