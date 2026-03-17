# HTTP API

`threadplane-server` is the write boundary for the POC. The CLI is a thin wrapper over these endpoints.
On startup, the server applies migrations, replays any unprojected events into Neo4j, and then keeps a background replay worker running with persisted projection offsets in PostgreSQL.

Base URL example:

```text
http://127.0.0.1:4000
```

## Basic Endpoints

`GET /`
- Returns the high-level service snapshot, including build metadata and dirty-worktree state for the running server binary.

`GET /healthz`
- Returns a simple health payload plus build metadata and dirty-worktree state for the running server binary.

`GET /scope`
- Returns the current POC summary plus build metadata and dirty-worktree state for the running server binary.

## Notes

`POST /v1/notes`
- Create a note.

Request:

```json
{
  "workspace": "shared-lab",
  "author": "agent-a",
  "title": "Lease design note",
  "body": "Claims should expire and return tasks to the pool."
}
```

`POST /v1/notes/update`
- Update a note and propagate through Xanadu links when present.

`GET /v1/notes/{note_id}`
- Fetch a note by ID.

## Epics

`POST /v1/epics`
- Create a first-class epic.

Request:

```json
{
  "workspace": "shared-lab",
  "author": "operator",
  "title": "Workflow foundations",
  "body": "Shared backlog for the workspace."
}
```

`GET /v1/epics/{epic_id}`
- Fetch an epic by ID.

`GET /v1/workspaces/{workspace}/epics`
- List epics for a workspace.

## Tasks

`POST /v1/tasks/offers`
- Offer a new task into a workspace. Tasks can optionally attach to an epic and declare direct dependencies at creation time.

Request:

```json
{
  "workspace": "shared-lab",
  "author": "operator",
  "epic_id": "00000000-0000-0000-0000-000000000000",
  "depends_on": ["11111111-1111-1111-1111-111111111111"],
  "priority": "high",
  "owner": "codex",
  "labels": ["agent", "workflow"],
  "title": "Investigate tuple leases",
  "details": "Need a shared lease-backed claim flow with dependency tracking."
}
```

`POST /v1/tasks/update`
- Update a task and propagate through Xanadu links when present. `epic_id` can also be supplied to attach the task to an epic.

Request:

```json
{
  "workspace": "shared-lab",
  "actor": "operator",
  "task_id": "00000000-0000-0000-0000-000000000000",
  "epic_id": "00000000-0000-0000-0000-000000000000",
  "priority": "urgent",
  "owner": "ops",
  "labels": ["sync", "xanadu"],
  "title": "Canonical lease wording",
  "details": "Updates from the task side should also rewrite the linked note."
}
```

`POST /v1/tasks/claim`
- Claim an open task with a lease.

Request:

```json
{
  "workspace": "shared-lab",
  "actor": "agent-b",
  "task_id": "00000000-0000-0000-0000-000000000000",
  "lease_seconds": 120
}
```

`POST /v1/tasks/release`
- Release an active claim and move the task back to `open`.

`POST /v1/tasks/complete`
- Mark a task `completed` and release any active claim.

`POST /v1/tasks/dependencies`
- Declare a direct `task -> depends_on -> task` edge. The server rejects edges that would create a cycle.

Request:

```json
{
  "workspace": "shared-lab",
  "actor": "operator",
  "task_id": "00000000-0000-0000-0000-000000000000",
  "depends_on_task_id": "11111111-1111-1111-1111-111111111111"
}
```

`GET /v1/workspaces/{workspace}/tasks`
- List tasks with optional filters:
  - `status=open|claimed|completed`
  - `epic_id=<uuid>`
  - `priority=low|medium|high|urgent`
  - `owner=<string>`
  - `label=<normalized-label>`
  - `limit=1..200`
  - `ready_only=true|false`

`GET /v1/workspaces/{workspace}/tasks/open`
- Convenience view for open tasks.

`GET /v1/tasks/{task_id}`
- Fetch the task record without graph context.

`GET /v1/tasks/{task_id}/context`
- Fetch a task, its epic, direct dependencies, direct dependents, readiness, active claim, and graph-linked relations.

`GET /v1/tasks/{task_id}/dag`
- Fetch the task plus its transitive dependency and dependent chains.

## Links

`POST /v1/links`
- Create a semantic graph link.

Request:

```json
{
  "workspace": "shared-lab",
  "actor": "agent-a",
  "from": "task:00000000-0000-0000-0000-000000000000",
  "relation": "depends_on",
  "to": "note:00000000-0000-0000-0000-000000000000"
}
```

`POST /v1/links/xanadu`
- Create a Xanadu transclusion link between text entities.

Request:

```json
{
  "workspace": "shared-lab",
  "actor": "agent-a",
  "from": "task:00000000-0000-0000-0000-000000000000",
  "to": "note:00000000-0000-0000-0000-000000000000"
}
```

## Events

`GET /v1/workspaces/{workspace}/events?limit=25`
- Fetch recent events for a workspace.

## Response Shape

Successful mutating and query endpoints return an envelope like:

```json
{
  "ok": true,
  "data": {}
}
```

Errors return:

```json
{
  "ok": false,
  "error": "human-readable message"
}
```

## Try It Quickly

The easiest way to exercise the whole API is still:

```bash
./scripts/e2e.sh
```

That script drives the live server through the CLI, which in turn exercises the endpoints above.
