# Architecture

## High-Level Position

`threadplane` is built around a strict service boundary:

- clients never write directly to the graph database
- clients never treat the graph as the source of truth
- all durable changes enter as commands handled by `threadplane-server`

The service validates commands, appends immutable events to PostgreSQL, then updates projections for query workloads. In the current shape, writes still project synchronously for low-latency reads, and a background replay worker keeps Neo4j catch-up durable through persisted projection offsets.

## Why This Shape

The product needs all of these properties at once:

- internet reachability
- durable audit trail
- explicit provenance
- graph-native queries for dependencies and related knowledge
- asynchronous handoff between agents

No single storage layer gives all of that cleanly. The system therefore separates responsibilities.

## Storage Planes

### 1. Authoritative Event Plane

Technology: PostgreSQL

Responsibilities:

- append immutable events
- store command receipts and idempotency keys
- persist tuple offers and leases
- support replay into downstream projections
- provide reliable transactional semantics
- evolve schema through versioned `sqlx` migrations

Why PostgreSQL:

- battle-tested operational model
- easy to host behind a network-facing service
- strong transactional guarantees
- good enough JSON support for evolving event payloads
- fits event sourcing well without special infrastructure

Expected tables in the first real implementation:

- `events`
- `commands`
- `workspaces`
- `actors`
- `task_leases`
- `projection_offsets`

### 2. Graph Projection Plane

Technology: Neo4j

Responsibilities:

- represent notes, tasks, facts, and decisions as nodes
- represent dependencies, provenance, and relatedness as edges
- support neighborhood queries and path traversal
- power "what is relevant context for this task?" style questions

Why Neo4j:

- widely understood property graph model
- strong fit for relationship-heavy queries
- mature tooling for local development and hosted options

The graph is not the source of truth. It is a projection that can always be rebuilt from events.

### 3. Coordination Plane

Technology: service-managed semantics with PostgreSQL persistence

Responsibilities:

- publish available work
- allow lease-based claiming
- expire abandoned claims
- expose read and take style APIs without exposing raw storage internals

This is intentionally not a generic raw tuple space in the first version. The service will implement a constrained, typed tuple model. That gives us most of the coordination benefits while keeping the system auditable and queryable.

## Tuple-Space Interpretation

Tuple spaces are a coordination pattern, not the source of truth.

In `threadplane`, that means:

- an offered task or observation is represented as a typed event
- the "open tuple" state is a projection derived from events plus lease state
- a `claim` command atomically creates a lease in PostgreSQL
- the graph can later incorporate accepted outcomes and dependencies

This keeps the blackboard behavior without letting transient coordination overwhelm the durable knowledge model.

## Command Model

Every durable action follows the same path:

1. Client sends a command to `threadplane-server`.
2. Service authenticates the actor and checks workspace policy.
3. Service validates payload shape and idempotency key.
4. Service appends one or more events in PostgreSQL.
5. Service updates projections synchronously on the write path and records enough state to replay them later.
6. Query models become visible through HTTP, CLI, or MCP.

The current POC keeps synchronous projection for fast local reads, but it also persists a `projection_offsets` watermark and runs a replay worker so the graph can be rebuilt or caught up after outages without changing the command contract.

## Initial Command Vocabulary

- `record_note`
- `link_entities`
- `offer_task`
- `claim_task`
- `release_task`
- `complete_task`
- `promote_fact`

## Initial Event Vocabulary

- `note_recorded`
- `link_declared`
- `task_offered`
- `task_claimed`
- `task_released`
- `task_completed`
- `fact_promoted`

## Graph Modeling Direction

Example node labels:

- `Workspace`
- `Actor`
- `Note`
- `Task`
- `Fact`
- `Decision`

Example edge types:

- `AUTHORED`
- `RECORDED_IN`
- `RELATED_TO`
- `DEPENDS_ON`
- `BLOCKS`
- `DERIVED_FROM`
- `PROMOTED_FROM`
- `CLAIMED_BY`

Bidirectional linking is handled by explicitly storing edge semantics instead of silently duplicating data. For example:

- `Task A DEPENDS_ON Task B`
- graph queries can also derive the inverse perspective:
  "Task B blocks Task A"

We should be careful to distinguish:

- explicitly asserted edges
- derived inverse views
- promoted facts that reached higher trust

## Provenance Model

Every queryable object should preserve:

- who wrote it
- when it was recorded
- what event created it
- whether it is speculative, accepted, or derived

For promoted knowledge, the graph node should include pointers back to all supporting event IDs. This is essential if agents are going to consume shared memory safely.

## Why VarveDB Is Still Relevant

VarveDB is not the initial shared system of record, but it remains relevant in three possible future roles:

- local write-ahead cache for intermittently connected agents
- embedded per-agent mirror for fast local replay
- specialized replication or archival component

Its append-only event store design is aligned with the product philosophy. The mismatch is deployment model, not conceptual fit.

## Deployment Model

For the POC:

- one `threadplane-server`
- one PostgreSQL instance
- one Neo4j instance

For a later hosted deployment:

- stateless API servers
- PostgreSQL HA or managed service
- managed Neo4j or equivalent graph platform
- background projector workers

## Security And Tenancy

The POC should model workspace isolation early, even if auth is basic.

Minimum expectations:

- actor identity included on every command
- workspace-scoped authorization checks
- idempotency keys on mutating commands
- no direct database access from agents

Future additions:

- signed agent tokens
- human-private vs shared visibility
- audit exports
- policy-based promotion rights

## Query Surfaces

The first code path should be HTTP because it is the most explicit and easiest to test. We should still design the surface so it maps cleanly to:

- CLI commands
- MCP tools
- possible future web UI

Key read patterns:

- recent workspace activity
- entity neighborhood and context graph
- open tasks and active claims
- dependency tree or blockers
- provenance for a fact or summary

## POC Recommendation

Do not build the whole dream at once.

Build this narrow slice first:

- a CLI that can record notes, offer tasks, claim tasks, and link entities
- a server that appends events and exposes current state
- a projector that maintains a small Neo4j graph
- one query that proves the concept:
  "show me everything relevant to this task, including blockers, recent notes, and supporting facts"

If that query feels powerful in real use, the architecture is worth extending.
