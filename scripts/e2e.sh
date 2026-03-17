#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WORKSPACE="e2e-$(date +%s)"
PROJECT="threadplane-${WORKSPACE}"
SERVER_LOG="$(mktemp)"
CONFIG_FILE="$(mktemp)"
SERVER_PID=""

pick_port() {
    python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
}

postgres_password="$(openssl rand -hex 18)"
neo4j_password="$(openssl rand -hex 18)"
postgres_port="$(pick_port)"
neo4j_http_port="$(pick_port)"
neo4j_bolt_port="$(pick_port)"
server_port="$(pick_port)"

export POSTGRES_DB="threadplane"
export POSTGRES_USER="threadplane"
export POSTGRES_PASSWORD="${postgres_password}"
export POSTGRES_PORT="${postgres_port}"
export NEO4J_USER="neo4j"
export NEO4J_PASSWORD="${neo4j_password}"
export NEO4J_HTTP_PORT="${neo4j_http_port}"
export NEO4J_BOLT_PORT="${neo4j_bolt_port}"
export THREADPLANE_CONFIG="${CONFIG_FILE}"

cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
        kill "$SERVER_PID" >/dev/null 2>&1 || true
        wait "$SERVER_PID" >/dev/null 2>&1 || true
    fi
    rm -f "$CONFIG_FILE"
    docker compose -p "$PROJECT" down -v >/dev/null 2>&1 || true
}

trap cleanup EXIT

cat >"$CONFIG_FILE" <<EOF
[cli]
url = "http://127.0.0.1:${server_port}"

[server]
bind = "127.0.0.1:${server_port}"
database_url = "postgres://threadplane:${postgres_password}@127.0.0.1:${postgres_port}/threadplane"
default_lease_seconds = 300
neo4j_password = "${neo4j_password}"
neo4j_uri = "127.0.0.1:${neo4j_bolt_port}"
neo4j_user = "neo4j"
EOF

docker compose -p "$PROJECT" up -d postgres neo4j >/dev/null

until docker compose -p "$PROJECT" exec -T postgres pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; do
    sleep 1
done

until curl -sf "http://127.0.0.1:${NEO4J_HTTP_PORT}" >/dev/null; do
    sleep 1
done

cargo run -q -p threadplane-server >"$SERVER_LOG" 2>&1 &
SERVER_PID="$!"

until curl -sf "http://127.0.0.1:${server_port}/healthz" >/dev/null; do
    if ! kill -0 "$SERVER_PID" >/dev/null 2>&1; then
        cat "$SERVER_LOG"
        exit 1
    fi
    sleep 1
done

epic_json="$(
    cargo run -q -p threadplane-cli -- \
        epic add \
        --workspace "$WORKSPACE" \
        --author operator \
        --title "Workflow foundations" \
        --body "Dogfood first-class epics and task DAGs."
)"
epic_id="$(jq -r '.data.epic_id' <<<"$epic_json")"

dependency_task_json="$(
    cargo run -q -p threadplane-cli -- \
        task offer \
        --workspace "$WORKSPACE" \
        --author operator \
        --epic-id "$epic_id" \
        --title "Ship durable task lifecycle" \
        --details "Completion should unlock dependent work."
)"
dependency_task_id="$(jq -r '.data.task_id' <<<"$dependency_task_json")"

task_json="$(
    cargo run -q -p threadplane-cli -- \
        task offer \
        --workspace "$WORKSPACE" \
        --author operator \
        --epic-id "$epic_id" \
        --depends-on "$dependency_task_id" \
        --title "Investigate tuple leases" \
        --details "Need a shared lease-backed claim flow with dependency tracking."
)"
task_id="$(jq -r '.data.task_id' <<<"$task_json")"
task_ref="$(jq -r '.data.entity_ref' <<<"$task_json")"

task_show_json="$(
    cargo run -q -p threadplane-cli -- \
        task show \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.title' <<<"$task_show_json")" == "Investigate tuple leases" ]]
[[ "$(jq -r '.data.details' <<<"$task_show_json")" == "Need a shared lease-backed claim flow with dependency tracking." ]]

task_list_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --status open
)"
[[ "$(jq -r '.data | length' <<<"$task_list_json")" == "2" ]]
[[ "$(jq -r --arg task_id "$task_id" '.data[] | select(.task.task_id == $task_id) | .ready' <<<"$task_list_json")" == "false" ]]
[[ "$(jq -r --arg task_id "$task_id" '.data[] | select(.task.task_id == $task_id) | .epic.epic_id' <<<"$task_list_json")" == "$epic_id" ]]

dag_json="$(
    cargo run -q -p threadplane-cli -- \
        task dag \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.dependencies[0].task_id' <<<"$dag_json")" == "$dependency_task_id" ]]
[[ "$(jq -r '.data.dependencies[0].depth' <<<"$dag_json")" == "1" ]]

complete_dependency_json="$(
    cargo run -q -p threadplane-cli -- \
        task complete \
        --workspace "$WORKSPACE" \
        --actor operator \
        --task-id "$dependency_task_id"
)"
[[ "$(jq -r '.data.status' <<<"$complete_dependency_json")" == "completed" ]]

ready_tasks_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --status open \
        --ready-only
)"
[[ "$(jq -r '.data | length' <<<"$ready_tasks_json")" == "1" ]]
[[ "$(jq -r '.data[0].task.task_id' <<<"$ready_tasks_json")" == "$task_id" ]]

compact_ready_queue="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --status open \
        --ready-only \
        --limit 1 \
        --format compact
)"
[[ "$compact_ready_queue" == *"Canonical lease wording"* || "$compact_ready_queue" == *"Investigate tuple leases"* ]]
[[ "$compact_ready_queue" == *"ready"* ]]

note_json="$(
    cargo run -q -p threadplane-cli -- \
        note add \
        --workspace "$WORKSPACE" \
        --author agent-a \
        --title "Lease design note" \
        --body "Claims should expire and return tasks to the pool."
)"
note_ref="$(jq -r '.data.entity_ref' <<<"$note_json")"

link_json="$(
    cargo run -q -p threadplane-cli -- \
        link xanadu \
        --workspace "$WORKSPACE" \
        --actor agent-a \
        --from "$task_ref" \
        --to "$note_ref"
)"
transclusion_id="$(jq -r '.data.transclusion_id' <<<"$link_json")"
[[ "$transclusion_id" != "null" ]]

note_update_json="$(
    cargo run -q -p threadplane-cli -- \
        note update \
        --workspace "$WORKSPACE" \
        --actor agent-a \
        --note-id "$(jq -r '.data.note_id' <<<"$note_json")" \
        --title "Lease semantics updated" \
        --body "A xanadu link should keep linked task text synchronized."
)"

context_after_note_update_json="$(
    cargo run -q -p threadplane-cli -- \
        task context \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.task.title' <<<"$context_after_note_update_json")" == "Lease semantics updated" ]]
[[ "$(jq -r '.data.task.details' <<<"$context_after_note_update_json")" == "A xanadu link should keep linked task text synchronized." ]]
[[ "$(jq -r '.data.task.transclusion_id' <<<"$context_after_note_update_json")" == "$transclusion_id" ]]
[[ "$(jq -r --arg note_ref "$note_ref" '.data.relations[] | select(.entity_ref == $note_ref) | .relation' <<<"$context_after_note_update_json")" == "XANADU_LINK" ]]
[[ "$(jq -r --arg note_ref "$note_ref" '.data.relations[] | select(.entity_ref == $note_ref) | .transclusion_id' <<<"$context_after_note_update_json")" == "$transclusion_id" ]]

task_update_json="$(
    cargo run -q -p threadplane-cli -- \
        task update \
        --workspace "$WORKSPACE" \
        --actor operator \
        --task-id "$task_id" \
        --epic-id "$epic_id" \
        --title "Canonical lease wording" \
        --details "Updates from the task side should also rewrite the linked note."
)"

note_after_task_update_json="$(
    cargo run -q -p threadplane-cli -- \
        note show \
        --note-id "$(jq -r '.data.note_id' <<<"$note_json")"
)"
[[ "$(jq -r '.data.title' <<<"$note_after_task_update_json")" == "Canonical lease wording" ]]
[[ "$(jq -r '.data.body' <<<"$note_after_task_update_json")" == "Updates from the task side should also rewrite the linked note." ]]
[[ "$(jq -r '.data.transclusion_id' <<<"$note_after_task_update_json")" == "$transclusion_id" ]]

triage_task_a_json="$(
    cargo run -q -p threadplane-cli -- \
        task offer \
        --workspace "$WORKSPACE" \
        --author operator \
        --title "Backfill roadmap item A" \
        --details "Needs bulk triage support."
)"
triage_task_a_id="$(jq -r '.data.task_id' <<<"$triage_task_a_json")"

triage_task_b_json="$(
    cargo run -q -p threadplane-cli -- \
        task offer \
        --workspace "$WORKSPACE" \
        --author operator \
        --title "Backfill roadmap item B" \
        --details "Needs bulk triage support."
)"
triage_task_b_id="$(jq -r '.data.task_id' <<<"$triage_task_b_json")"

triage_json="$(
    cargo run -q -p threadplane-cli -- \
        task triage \
        --workspace "$WORKSPACE" \
        --actor operator \
        --epic-id "$epic_id" \
        --complete \
        --task-id "$triage_task_a_id" \
        --task-id "$triage_task_b_id"
)"
[[ "$(jq -r '.completed_task_ids | length' <<<"$triage_json")" == "2" ]]
[[ "$(jq -r '.updated_task_ids | length' <<<"$triage_json")" == "2" ]]
[[ "$(jq -r '.unchanged_task_ids | length' <<<"$triage_json")" == "0" ]]

context_before_claim_json="$(
    cargo run -q -p threadplane-cli -- \
        task context \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.ready' <<<"$context_before_claim_json")" == "true" ]]
[[ "$(jq -r '.data.epic.epic_id' <<<"$context_before_claim_json")" == "$epic_id" ]]
[[ "$(jq -r '.data.dependencies[0].task_id' <<<"$context_before_claim_json")" == "$dependency_task_id" ]]

claim_json="$(
    cargo run -q -p threadplane-cli -- \
        task claim \
        --workspace "$WORKSPACE" \
        --actor agent-b \
        --task-id "$task_id" \
        --lease-seconds 120
)"
[[ "$(jq -r '.data.actor' <<<"$claim_json")" == "agent-b" ]]

claimed_tasks_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --status claimed
)"
[[ "$(jq -r '.data | length' <<<"$claimed_tasks_json")" == "1" ]]
[[ "$(jq -r '.data[0].task.task_id' <<<"$claimed_tasks_json")" == "$task_id" ]]

released_task_json="$(
    cargo run -q -p threadplane-cli -- \
        task release \
        --workspace "$WORKSPACE" \
        --actor agent-b \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.status' <<<"$released_task_json")" == "open" ]]

reclaimed_json="$(
    cargo run -q -p threadplane-cli -- \
        task claim \
        --workspace "$WORKSPACE" \
        --actor agent-b \
        --task-id "$task_id" \
        --lease-seconds 120
)"
[[ "$(jq -r '.data.actor' <<<"$reclaimed_json")" == "agent-b" ]]

completed_task_json="$(
    cargo run -q -p threadplane-cli -- \
        task complete \
        --workspace "$WORKSPACE" \
        --actor agent-b \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.status' <<<"$completed_task_json")" == "completed" ]]

context_json="$(
    cargo run -q -p threadplane-cli -- \
        task context \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.active_claim' <<<"$context_json")" == "null" ]]
[[ "$(jq -r --arg note_ref "$note_ref" '.data.relations[] | select(.entity_ref == $note_ref) | .entity_ref' <<<"$context_json")" == "$note_ref" ]]
[[ "$(jq -r '.data.task.title' <<<"$context_json")" == "Canonical lease wording" ]]
[[ "$(jq -r '.data.task.status' <<<"$context_json")" == "completed" ]]

events_json="$(
    cargo run -q -p threadplane-cli -- \
        events list \
        --workspace "$WORKSPACE" \
        --limit 20
)"
[[ "$(jq -r '.data[0].kind' <<<"$events_json")" == "task_completed" ]]
[[ "$(jq -r '.data[] | select(.kind == "task_dependency_declared") | .kind' <<<"$events_json" | head -n1)" == "task_dependency_declared" ]]
[[ "$(jq -r '.data[] | select(.kind == "epic_recorded") | .kind' <<<"$events_json" | head -n1)" == "epic_recorded" ]]

echo "threadplane e2e ok"
echo "workspace=$WORKSPACE"
echo "$epic_json"
echo "$dependency_task_json"
echo "$task_json"
echo "$note_json"
echo "$link_json"
echo "$note_update_json"
echo "$task_update_json"
echo "$claim_json"
echo "$released_task_json"
echo "$completed_task_json"
