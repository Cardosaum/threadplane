# ADR 0001: PostgreSQL Event Log + Neo4j Projection

## Status

Accepted for the POC.

## Context

The product needs:

- a network-reachable source of truth
- durable append-only history
- graph-native traversal for dependencies and shared knowledge
- support for tuple-space inspired coordination

VarveDB was considered because it matches the event-sourcing philosophy well. The issue is not capability, but deployment shape: it is embedded storage, while the product fundamentally requires a shared service boundary for remote clients and agents.

## Decision

For the POC:

- PostgreSQL is the authoritative event log
- Neo4j is a rebuildable graph projection
- tuple-space semantics are implemented by the service and persisted in PostgreSQL

## Rationale

- PostgreSQL is widely available, operationally familiar, and strong enough for an append-only event store.
- Neo4j gives us the graph traversal model we need without pretending the graph should also be the source of truth.
- Constraining tuple semantics to the service reduces chaos and keeps collaboration state auditable.
- This stack is concrete enough to build now and flexible enough to evolve later.

## Consequences

Positive:

- clear separation between write authority and query projections
- easier provenance and replay story
- internet-facing deployment model is straightforward
- no need to expose raw DB semantics to agents

Negative:

- two storage systems instead of one
- projection lag becomes a real concern as the system grows
- some coordination semantics that would feel "native" in a true tuple space must be modeled explicitly

## Deferred Options

- use VarveDB as an offline-first local event mirror
- move tuple coordination onto a dedicated messaging substrate later
- evaluate alternate graph stores after the first real workload

