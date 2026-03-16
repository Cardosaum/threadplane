# POC Scope

## One-Sentence Summary

Build a shared memory and coordination service that lets humans and AI agents publish notes, task offers, claims, links, and dependency updates into an internet-reachable system with a durable event log and a queryable graph.

## Problem Statement

Teams increasingly have multiple agents doing useful work in parallel, but most agent memory systems break down in one of two ways:

- they are local or per-user, so collaboration becomes copy-paste or chat archaeology
- they are shared, but they flatten knowledge into blobs instead of preserving links, dependencies, and provenance

The missing piece is a service that treats agent collaboration as a first-class workload:

- notes should be shareable across people and agents
- tasks should be discoverable and claimable asynchronously
- provenance should survive summarization and promotion
- dependencies should be explicit and traversable
- the system of record should be reachable over the internet and robust under normal production operations

## POC Goal

Prove that one service can successfully combine:

- append-only events for auditability and replay
- graph materialization for knowledge and dependency queries
- tuple-space inspired coordination for work discovery and handoff
- CLI-first ergonomics for both humans and agents

## Primary Users

- individual builders coordinating multiple local and remote agents
- small teams that want shared operational memory across humans and agents
- code or research agents that need a reliable place to leave context for other agents

## POC User Stories

- A coding agent records a note about a failed migration and links it to the task it blocked.
- A second agent queries open work in a workspace and claims a task without human intervention.
- A teammate inspects the dependency graph and understands why a task is blocked.
- A human promotes a speculative note into an accepted fact while preserving provenance.
- Another person's agents can fetch the latest relevant context without parsing chat history.

## In Scope

- internet-reachable service boundary with authenticated writes
- workspaces and actor identities
- append-only event log as the source of truth
- graph projection for entities and relationships
- tuple-space style primitives:
  `offer`, `read`, `claim`, `release`, `complete`, `expire`
- note and link primitives:
  `record_note`, `link_entities`, `promote_fact`
- dependency tracking between tasks, notes, facts, and decisions
- CLI and HTTP API scaffolding
- MCP adapter design, even if the first code path is HTTP only

## Out Of Scope For The POC

- semantic search and embeddings beyond placeholders
- large-scale ingestion pipelines
- fully offline-first replication
- enterprise auth integrations
- conflict-free collaborative editing
- advanced ranking and recommendation logic
- deep workflow automation engines

## Core Entities

- `workspace`: tenancy boundary for a team, project, or collaboration context
- `actor`: human or agent identity that emits events
- `note`: an observation, handoff, summary, or decision record
- `task`: a unit of work that can be offered, claimed, blocked, or completed
- `fact`: a promoted, higher-trust knowledge object
- `link`: a typed relation between entities
- `claim`: a lease granting temporary ownership of a task or tuple
- `event`: immutable source-of-truth record

## Success Criteria

- A write enters through one service boundary and lands durably in PostgreSQL.
- The same write becomes visible as graph state in Neo4j.
- Two actors can collaborate in the same workspace without direct coordination.
- A task can be claimed with a lease and safely returned to the pool after expiry.
- Every graph fact shown to users has traceable provenance back to one or more events.
- The CLI is simple enough for an agent to use non-interactively.

## Non-Goals

- replacing issue trackers
- replacing a source code forge
- becoming a general chat platform
- modeling every possible collaboration artifact in v1

## Proposed POC Milestones

### Milestone 0: shape the system

- finalize bounded vocabulary and event taxonomy
- stand up PostgreSQL and Neo4j locally
- expose a server with health and scope endpoints

### Milestone 1: basic shared memory

- `record_note`
- `link_entities`
- `list_recent_events`
- `get_entity_context`

### Milestone 2: tuple-space coordination

- `offer_task`
- `claim_task`
- lease expiry and reclaim
- `list_open_tasks`

### Milestone 3: graph-backed dependency views

- `depends_on`
- `blocks`
- `derived_from`
- `related_to`
- dependency traversal queries

### Milestone 4: trust and promotion

- `promote_fact`
- provenance display
- accepted vs speculative state

## Open Questions

- Should accepted facts be immutable snapshots or mutable graph state derived from events?
- Do we need separate namespaces for human-private, agent-private, and workspace-shared writes from day one?
- Should a claim be exclusive by default, or can multiple agents subscribe as watchers?
- How much tuple matching should be query-based versus exact type-based in the POC?
- Is Neo4j the best first graph projection, or would Memgraph/FalkorDB improve operational fit later?

