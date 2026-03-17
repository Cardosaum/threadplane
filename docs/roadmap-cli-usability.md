# CLI Usability Roadmap

`threadplane` already supports the core shared-workflow loop, but it still asks too much of both humans and autonomous agents in day-to-day usage. This roadmap turns the current gap list into a phased plan that prioritizes composable primitives first, then richer operator ergonomics on top.

## Goals

- make the CLI strong enough for real dogfooding by humans and AI agents
- reduce the need to remember opaque IDs when discovering work or context
- prefer reusable read/write abstractions over one-off commands
- keep machine-readable output stable while improving human-readable affordances

## Design Principles

- build bottom up: shared server/core abstractions first, then CLI commands
- prefer one good primitive over multiple overlapping convenience surfaces
- make machine use first-class, not an afterthought
- keep commands composable and idempotent where possible

## Current Strengths

- first-class epics, tasks, dependencies, leases, completion, and triage
- durable event log in PostgreSQL and graph projection in Neo4j
- task dependency and context inspection
- Xanadu-style shared transclusion updates across linked notes/tasks
- strong configuration, build metadata, and replay-status introspection

## Gaps

### 1. Queue Selection And Assignment

The CLI can list ready work, but there is no first-class "what should I do next?" command and no direct "claim the next best task" operation for agents.

Desired outcome:

- `task next`
- `task claim-next`
- consistent priority-aware ready queue semantics

### 2. Note Discovery

Notes are writeable and addressable by UUID, but there is no first-class list/search flow. That makes knowledge discovery awkward for both humans and agents.

Desired outcome:

- `note list`
- `note search`
- reusable server-side note query abstraction

### 3. Incremental Event Consumption

The CLI can list recent events, but it cannot consume changes incrementally or follow a workspace over time.

Desired outcome:

- cursor-based event reads
- `events tail`
- `events tail --follow`

### 4. Graph And Entity Discovery

Links are writable, but the read surface is still task-centric. Users and agents need better ways to ask "what is related to this entity?" without already knowing the exact task context endpoint to call.

Desired outcome:

- entity-centric relation exploration
- link listing and related-entity discovery
- graph reads that work for notes, tasks, and epics

### 5. Agent-Oriented Read Models

JSON output exists, but many operations still return generic envelopes rather than purpose-built machine views for planning and execution.

Desired outcome:

- stable, explicit machine-oriented views for queue selection and context handoff
- clear distinction between human summary output and machine output

### 6. Multi-User And Agent Identity

The system is still too trusting for internet-facing shared usage.

Desired outcome:

- workspace-scoped auth
- clearer actor identity and ownership semantics

### 7. MCP Surface

The CLI is usable, but AI agents still need an integration layer that exposes the same capabilities through MCP.

Desired outcome:

- MCP adapter over the same command model
- no duplicate business logic

## Phases

### Phase 1. Discovery And Queue Primitives

Scope:

- add `task next`
- add `task claim-next`
- add note listing and note search
- add cursor-based event reads and `events tail`

Why first:

- these commands make the existing workflow materially easier to use
- they unlock higher-level automation without introducing a lot of new domain concepts

### Phase 2. Entity Exploration

Scope:

- entity-oriented related-entity reads
- link listing and richer graph traversal
- stronger note/task/epic cross-navigation

### Phase 3. Machine Contracts

Scope:

- purpose-built machine views
- clearer compact vs JSON contracts
- explicit handoff/summarization surfaces

### Phase 4. Identity And Integration

Scope:

- auth
- agent identity semantics
- MCP adapter

## Current Implementation Status

In progress:

- Phase 1 queue selection
- Phase 1 note discovery
- Phase 1 incremental event consumption

Not started:

- Phase 2 entity exploration
- Phase 3 machine contracts
- Phase 4 identity and integration
