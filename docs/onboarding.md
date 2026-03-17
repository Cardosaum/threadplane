# Onboarding

This guide gets you from a fresh clone to a working shared workspace in a few minutes.

## What You Need

- Rust toolchain
- Docker and Docker Compose
- `jq` for easier shell scripting during manual testing

## First Run

1. Generate local config and credentials:

```bash
./scripts/generate-env.sh
```

2. Start PostgreSQL and Neo4j:

```bash
docker compose up -d
```

3. Start the server:

```bash
cargo run -p threadplane-server
```

4. In a second terminal, confirm the CLI can resolve config:

```bash
cargo run -p threadplane-cli -- config show
```

5. Inspect the service summary:

```bash
cargo run -p threadplane-cli -- scope
```

## First Shared Workflow

Offer a task:

```bash
cargo run -p threadplane-cli -- task offer \
  --workspace shared-lab \
  --author operator \
  --title "Investigate tuple leases" \
  --details "Need a shared lease-backed claim flow."
```

Create a note:

```bash
cargo run -p threadplane-cli -- note add \
  --workspace shared-lab \
  --author agent-a \
  --title "Lease design note" \
  --body "Claims should expire and return tasks to the pool."
```

Create a Xanadu link:

```bash
cargo run -p threadplane-cli -- link xanadu \
  --workspace shared-lab \
  --actor agent-a \
  --from task:<task-id> \
  --to note:<note-id>
```

Inspect graph-backed context:

```bash
cargo run -p threadplane-cli -- task context --task-id <task-id>
```

Claim the task:

```bash
cargo run -p threadplane-cli -- task claim \
  --workspace shared-lab \
  --actor agent-b \
  --task-id <task-id> \
  --lease-seconds 120
```

## Fastest Verification Path

If you want a single command that proves the vertical slice works end to end:

```bash
./scripts/e2e.sh
```

That script boots the local stack, starts the API server, runs the CLI against it, and verifies:

- task offers
- note creation
- Xanadu linking
- bidirectional content sync
- lease-backed claiming
- event history
- graph-backed task context

## Common Next Steps

- Read [configuration.md](./configuration.md) to switch config files or override settings.
- Read [http-api.md](./http-api.md) if you want to drive the service directly over HTTP.
- Read [architecture.md](./architecture.md) if you want the deeper model behind the POC.
