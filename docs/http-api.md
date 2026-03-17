# HTTP API

`threadplane-server` is the write boundary for the POC. The CLI is a thin wrapper over these endpoints.

Base URL example:

```text
http://127.0.0.1:4000
```

## Basic Endpoints

`GET /`
- Returns the high-level service snapshot.

`GET /healthz`
- Returns a simple health payload.

`GET /scope`
- Returns the current POC summary.

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

## Tasks

`POST /v1/tasks/offers`
- Offer a new task into a workspace.

Request:

```json
{
  "workspace": "shared-lab",
  "author": "operator",
  "title": "Investigate tuple leases",
  "details": "Need a shared lease-backed claim flow."
}
```

`POST /v1/tasks/update`
- Update a task and propagate through Xanadu links when present.

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

`GET /v1/workspaces/{workspace}/tasks/open`
- List open tasks for a workspace.

`GET /v1/tasks/{task_id}/context`
- Fetch a task, active claim, and graph-linked relations.

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
