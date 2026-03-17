# threadplane

[![threadplane demo](./docs/threadplane-demo.gif)](./docs/threadplane-demo.cast)

`threadplane` is a shared memory and coordination plane for people and AI agents: one internet-reachable service accepts writes, PostgreSQL keeps the durable event log, Neo4j exposes the traversable graph, and agents can leave work, notes, epics, task DAGs, and claims for each other to pick up later. The animation above is clickable and opens the source [asciinema cast](./docs/threadplane-demo.cast); regenerate both assets with `./scripts/regenerate-demo.sh`.

This repository is a working POC. It already demonstrates first-class epics, DAG-style task dependencies, lease-backed task lifecycle, graph links, and Xanadu-style transclusion where linked note/task text stays synchronized.

Read next:

- [Onboarding guide](./docs/onboarding.md)
- [Configuration guide](./docs/configuration.md)
- [Dependency policy](./docs/dependencies.md)
- [HTTP API reference](./docs/http-api.md)

## Why It Matters

Most agent tooling gets one part right and leaves the rest as an exercise:

- memory is local, but collaboration is weak
- collaboration exists, but knowledge is not graph-native
- knowledge is graph-native, but task handoff and dependency tracking are awkward
- there is an event log, but no CLI-first workflow for humans and agents sharing the same context

`threadplane` is meant to close that gap with a single control plane:

- shared context across people, CLIs, and agents
- durable append-only history
- graph-backed traversal of dependencies and provenance
- first-class epics and task DAGs for backlog structure
- lease-backed claiming so work can be discovered and handed off safely
- Xanadu-style links for shared text between notes and tasks

## What You Can Do Today

- Create epics as durable, first-class planning entities.
- Offer a task into a shared workspace.
- Declare task dependencies and inspect the task DAG.
- Ask for ready-only tasks whose dependencies are already completed.
- Add notes that other agents can find later.
- Create semantic links between entities.
- Create Xanadu links so note/task text is kept in sync.
- Claim open tasks with expiring leases.
- Release or complete tasks as work moves through the DAG.
- Inspect recent workspace events.
- Ask for task context enriched with graph-linked relations.

## Quick Start

1. Generate local config and credentials:

```bash
./scripts/generate-env.sh
```

That creates:

- `.env` for Docker Compose and local database credentials
- `etc/config.toml` for `threadplane-server` and `threadplane-cli`

2. Start PostgreSQL and Neo4j:

```bash
docker compose up -d
```

3. Start the API server:

```bash
cargo run -p threadplane-server
```

On startup, the server applies versioned `sqlx` migrations from [`crates/threadplane-server/migrations`](./crates/threadplane-server/migrations), catches the graph projection up from PostgreSQL, and keeps a replay worker running with persisted projection offsets. A fresh database bootstraps cleanly, and a stale or empty Neo4j graph can be rebuilt without inventing a second source of truth.

4. In another terminal, inspect the service:

```bash
cargo run -p threadplane-cli -- scope
cargo run -p threadplane-cli -- projection status
cargo run -p threadplane-cli -- config show
cargo run -p threadplane-cli -- build compare
```

`scope` now includes the running server build identity, including whether the binary came from a dirty worktree, so you can quickly confirm what your CLI is actually talking to.
It also includes the persisted graph replay watermark and pending projection count, and `projection status` exposes the same data directly when you want an operational read instead of the broader product summary.
If the local CLI build and running server build drift apart, `scope` prints a warning on stderr and `build compare` shows the full typed diff.
Mutating commands also accept a global `--idempotency-key <key>`, which maps to durable command receipts in PostgreSQL so retries can safely replay the original result instead of duplicating writes.

5. Run the full smoke test if you want the fastest proof that everything works:

```bash
./scripts/e2e.sh
```

For daily dogfooding, use the one-command local refresh flow:

```bash
./scripts/dogfood.sh up
```

## Two-Minute Walkthrough

Create an epic:

```bash
cargo run -p threadplane-cli -- epic add \
  --workspace shared-lab \
  --author operator \
  --title "Workflow foundations" \
  --body "Shared backlog for the workspace."
```

Create a dependency task:

```bash
cargo run -p threadplane-cli -- task offer \
  --workspace shared-lab \
  --author operator \
  --epic-id <epic-id> \
  --title "Ship durable task lifecycle" \
  --details "Completion should unlock dependent work."
```

Create a dependent task:

```bash
cargo run -p threadplane-cli -- task offer \
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
```

Inspect the DAG view:

```bash
cargo run -p threadplane-cli -- task show --task-id <task-id>
cargo run -p threadplane-cli -- task dag --task-id <task-id>
cargo run -p threadplane-cli -- task blocked-by --task-id <task-id>
cargo run -p threadplane-cli -- task blocks --task-id <dependency-task-id> --direct-only
```

Complete the prerequisite and ask for ready work:

```bash
cargo run -p threadplane-cli -- task complete \
  --workspace shared-lab \
  --actor operator \
  --task-id <dependency-task-id>

cargo run -p threadplane-cli -- task list \
  --workspace shared-lab \
  --status open \
  --ready-only \
  --limit 5 \
  --format compact
```

Ready queues come back ordered by priority first and then recency, so the first compact rows are the best next picks for people and agents.

Bulk-triage multiple backlog items at once:

```bash
cargo run -p threadplane-cli -- task triage \
  --workspace shared-lab \
  --actor operator \
  --epic-id <epic-id> \
  --priority low \
  --owner backlog \
  --label triaged \
  --complete \
  --task-id <task-a-id> \
  --task-id <task-b-id>
```

Create a note:

```bash
cargo run -p threadplane-cli -- note add \
  --workspace shared-lab \
  --author agent-a \
  --title "Lease design note" \
  --body "Claims should expire and return tasks to the pool."
```

Create a Xanadu link between them:

```bash
cargo run -p threadplane-cli -- link xanadu \
  --workspace shared-lab \
  --actor agent-a \
  --from task:<task-id> \
  --to note:<note-id>
```

Update one side and let the shared transclusion propagate:

```bash
cargo run -p threadplane-cli -- note update \
  --workspace shared-lab \
  --actor agent-a \
  --note-id <note-id> \
  --title "Lease semantics updated" \
  --body "A xanadu link should keep linked task text synchronized."
```

Inspect the task with graph-backed context:

```bash
cargo run -p threadplane-cli -- task context --task-id <task-id>
```

## CLI Surface

Current commands:

- `scope`
- `projection status`
- `build show`
- `build compare`
- `epic add`
- `epic list`
- `epic show`
- `note add`
- `note show`
- `note update`
- `task blocked-by`
- `task blocks`
- `task complete`
- `task offer`
- `task depend`
- `task dag`
- `task list`
  - supports `--limit`, `--format compact`, `--priority`, `--owner`, and `--label`
- `task update`
- `task claim`
- `task release`
- `task show`
- `task triage`
  - supports bulk epic assignment plus durable metadata updates
- `task context`
- `link add`
- `link xanadu`
- `events list`
- `config show`

Global options:

- `--config <path>` to load a specific config file
- `--server <url>` to override the configured API base URL
- `--idempotency-key <key>` to make a mutating command safely retryable

## Configuration

`threadplane` loads config in this order:

1. built-in defaults
2. repo-local `etc/config.toml`
3. system config at `/etc/threadplane/config.toml`
4. `THREADPLANE_CONFIG=/path/to/config.toml`
5. `THREADPLANE__...` nested environment overrides

See [etc/config.toml.example](./etc/config.toml.example) for the file format.

Inspect the resolved config and discovery order:

```bash
cargo run -p threadplane-cli -- config show
```

Use an explicit config file for a one-off run:

```bash
cargo run -p threadplane-cli -- \
  --config /path/to/config.toml \
  scope
```

Example:

```toml
[cli]
url = "http://127.0.0.1:4000"

[server]
bind = "127.0.0.1:4000"
database_url = "postgres://threadplane:password@127.0.0.1:5432/threadplane"
default_lease_seconds = 300
neo4j_password = "password"
neo4j_uri = "127.0.0.1:7687"
neo4j_user = "neo4j"
```

## Architecture At A Glance

- `threadplane-server` is the write boundary.
- PostgreSQL is the system of record.
- Neo4j is a rebuildable graph projection.
- PostgreSQL also stores projection replay offsets, so catch-up survives restarts.
- Tuple-space semantics live in the service layer.
- Tasks can belong to first-class epics and depend on other tasks without creating cycles.
- Claims use leases so work returns to the pool if an agent disappears.
- Xanadu links join textual entities into a shared transclusion group.

## Why PostgreSQL First

VarveDB is an important influence for the event-sourcing model, but this POC optimizes first for a shared internet-reachable source of truth. That makes PostgreSQL the practical first event log. If `threadplane` later grows local replicas or offline-first ingest, VarveDB is still an interesting building block for edge or sidecar use.

## Repository Layout

- `crates/threadplane-core`: shared types, config loading, and core helpers
- `crates/threadplane-server`: HTTP server and projections
- `crates/threadplane-cli`: human and agent CLI
- `compose.yaml`: local PostgreSQL and Neo4j stack
- `docs/poc-scope.md`: POC boundaries and success criteria
- `docs/onboarding.md`: first-run walkthrough for humans and agents
- `docs/configuration.md`: config precedence, examples, and troubleshooting
- `docs/http-api.md`: endpoint-level API reference
- `docs/architecture.md`: deeper architecture notes
- `docs/adr/0001-authoritative-log-and-graph-projection.md`: first ADR

## Current Status

This is a POC, not a finished platform. The current implementation proves the shape:

- end-to-end CLI flow works
- first-class epics and task DAGs work
- event log is durable in PostgreSQL
- mutating commands support idempotency keys and durable command receipts
- graph projection is live in Neo4j
- projection replay and recovery survive restarts
- task claims are lease-backed
- task release/complete lifecycle is live
- Xanadu linking propagates content between note/task pairs

The next layers are production concerns such as auth, richer multi-workspace policy, benchmark and stress tooling, and MCP-facing ergonomics.
