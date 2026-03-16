# threadplane

`threadplane` is a proposed shared memory and coordination service for human and AI agents.

The core idea is simple:

- one internet-reachable service receives writes from CLIs, MCP clients, and automations
- PostgreSQL stores the append-only event log and remains the system of record
- Neo4j materializes a graph for notes, dependencies, provenance, and bidirectional links
- tuple-space semantics live in the service so agents can discover, claim, and hand off work asynchronously

This repository is a POC scaffold, not a finished product. The initial goal is to validate the architecture and API shape before we invest in deeper replication, search, and policy work.

## Local Configuration

This repo does not commit reusable credentials. Generate a local `.env` before starting the stack:

```bash
./scripts/generate-env.sh
```

The generated `.env` is ignored by git. Docker Compose binds services to `127.0.0.1` by default so local database ports are not exposed on all interfaces.

## Why This Exists

Existing agent memory tools get close, but they tend to optimize for one of these axes at the expense of the others:

- local-first memory with weak shared collaboration
- shared memory without a first-class graph
- graph memory without clear task and dependency coordination
- event history without ergonomic CLI-first agent workflows

`threadplane` is an attempt to combine those pieces into one design that feels like "networked Beads for people and agents", while still leaning on battle-tested infrastructure.

## POC Direction

The POC deliberately keeps the number of moving parts small:

- `threadplane-server`: an HTTP control plane that accepts commands and exposes query endpoints
- `threadplane-cli`: a CLI used by humans and agents
- `PostgreSQL`: authoritative append-only event log, leases, and durable command state
- `Neo4j`: graph projection for knowledge, dependencies, and traversal

For the first iteration, tuple-space primitives are implemented in the service and persisted in PostgreSQL. That gives us internet reachability and operational simplicity without introducing a third coordination system on day one.

## Why Not VarveDB As The First Source Of Truth

VarveDB is a strong influence on the event-sourcing side of the design, especially the append-only log mindset. For this POC, though, it is the wrong default storage engine because the primary requirement is a shared, internet-reachable source of truth. VarveDB is embedded and excellent for in-process event storage, but this system needs a network-facing service boundary anyway.

That makes PostgreSQL the practical first choice for the shared event log. If the product later needs offline-first replicas, edge ingestion, or high-performance local mirrors, VarveDB remains an interesting building block for agents or sidecars.

## Repo Layout

- `crates/threadplane-core`: shared types and POC summaries
- `crates/threadplane-server`: minimal HTTP server scaffold
- `crates/threadplane-cli`: CLI scaffold for notes, tasks, and links
- `docs/poc-scope.md`: product scope, boundaries, and success criteria
- `docs/architecture.md`: deeper technical design
- `docs/adr/0001-authoritative-log-and-graph-projection.md`: first architecture decision record
- `compose.yaml`: local PostgreSQL + Neo4j stack for development

## Quick Start

1. Start local dependencies:

```bash
./scripts/generate-env.sh
docker compose up -d
```

2. Run the server:

```bash
cargo run -p threadplane-server
```

3. Inspect the current POC framing:

```bash
cargo run -p threadplane-cli -- scope
```

4. Create a task, a note, and inspect context:

```bash
cargo run -p threadplane-cli -- task offer \
  --workspace shared-lab \
  --author operator \
  --title "Investigate tuple leases" \
  --details "Need a shared lease-backed claim flow."

cargo run -p threadplane-cli -- note add \
  --workspace shared-lab \
  --author agent-a \
  --title "Agent handoff" \
  --body "Investigate dependency edge semantics."
```

5. Run the end-to-end smoke test:

```bash
./scripts/e2e.sh
```

The e2e script boots PostgreSQL + Neo4j with Docker Compose, starts `threadplane-server` on `127.0.0.1:4010`, runs the CLI against the live service, and verifies task offers, notes, links, claims, event history, and graph-backed task context.

## Initial Name Choice

The proposed project name is `threadplane`.

Why this one:

- `thread` matches discussion, work items, notes, and dependency chains
- `plane` suggests a shared coordination surface rather than a single database
- it works whether the dominant interaction is CLI, MCP, or web API

Alternatives worth keeping in reserve:

- `knotspace`
- `weftplane`
- `relaygraph`
