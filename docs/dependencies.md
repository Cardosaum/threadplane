# Dependency Policy

`threadplane` keeps dependency management intentionally boring:

- prefer current stable direct dependencies
- centralize versions in the workspace root
- enable the smallest useful feature set
- raise MSRV deliberately when a dependency requires it
- prove changes with tests, nightly Clippy, and the end-to-end smoke flow

This document is the source of truth for how the workspace chooses and evolves its crate set.

## Principles

### Latest Stable Direct Dependencies

Direct dependencies should stay on the latest stable release line unless there is a concrete compatibility reason not to.

That keeps the project closer to the current Rust ecosystem, reduces long-lived upgrade jumps, and makes security or bug-fix updates smaller.

### Workspace-Owned Versions

All shared crate versions live in the root [Cargo.toml](../Cargo.toml). Member crates should consume them through `.workspace = true` instead of pinning their own versions.

That gives us:

- one place to audit versions
- one place to tune feature flags
- one place to reason about MSRV impact

### Narrow Feature Flags

Default features stay off when a crate supports that cleanly and the extra surface is not needed.

The goal is to keep builds smaller, reduce transitive churn, and make the runtime surface easier to reason about.

Examples in the current workspace:

- `axum`: only `http1`, `json`, `query`, and `tokio`
- `reqwest`: only `blocking`, `json`, and `rustls`
- `sqlx`: only `chrono`, `derive`, `json`, `postgres`, `runtime-tokio-rustls`, and `uuid`
- `tracing-subscriber`: only `env-filter` and `fmt`
- `derive_more` and `strum`: derive macros only

### Deliberate MSRV

The workspace MSRV is declared in the root manifest:

- Rust `1.85`

We raise it only when a direct dependency forces the change or when the language/runtime gain clearly simplifies the codebase enough to justify the move.

When MSRV changes, update the root manifest and mention the reason in the same change that bumped the dependency set.

### Rustls by Default

Networked crates should prefer Rustls-backed TLS instead of OpenSSL-backed defaults unless there is a strong operational reason otherwise.

That keeps local setup simpler and makes the project easier to run in containers and CI.

## Current Direct Dependency Roles

- `axum`: HTTP routing and JSON API surface for `threadplane-server`
- `chrono`: timestamps in records and event payloads
- `clap`: CLI parsing and help UX for `threadplane-cli`
- `derive_more`: low-boilerplate constructors and display impls
- `figment`: layered config loading from explicit TOML selection plus nested env overrides
- `neo4rs`: Neo4j projection client
- `reqwest`: blocking HTTP client for the CLI
- `rstest`: table-driven unit tests
- `proptest`: property-based tests for core invariants
- `serde` and `serde_json`: transport and persistence payload encoding
- `snafu`: typed error handling with context
- `sqlx`: PostgreSQL access
- `strum`: enum derive helpers
- `tokio` and `tokio-util`: async runtime and cancellation primitives
- `tracing` and `tracing-subscriber`: structured logs
- `uuid`: stable entity identifiers

## Update Workflow

When changing dependency versions or feature flags:

1. Edit the root [Cargo.toml](../Cargo.toml).
2. Keep member crates on workspace-managed dependencies where possible.
3. Prefer removing feature flags before adding new ones.
4. Confirm the resulting API or runtime need is documented if the change is user-visible.

Then verify:

```bash
cargo test --workspace
cargo +nightly clippy --workspace --all-targets
./scripts/e2e.sh
```

If the change affects the demo or onboarding flow, also refresh the README-facing assets or docs in the same slice.

## What We Avoid

- member-crate version drift
- broad default feature sets without a reason
- compatibility shims for old config or runtime behavior in this greenfield codebase
- undocumented MSRV bumps
- dependency additions that duplicate an existing building block

## Practical Heuristic

Before adding a crate, ask:

1. Does the workspace already have a crate that solves this well enough?
2. Can a small local abstraction do the job with less surface area?
3. If a new crate is justified, can it be added with a narrow feature set and a clear owner role?

If the answer to those questions is still "yes, add it", document the role and keep the integration small.
