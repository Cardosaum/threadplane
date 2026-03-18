#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WORKSPACE="e2e-$(date +%s)"
PROJECT="threadplane-${WORKSPACE}"
SERVER_LOG="$(mktemp)"
XDG_CONFIG_HOME="$(mktemp -d)"
CONFIG_FILE="${XDG_CONFIG_HOME}/threadplane/config.toml"
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
export XDG_CONFIG_HOME

cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
        kill "$SERVER_PID" >/dev/null 2>&1 || true
        wait "$SERVER_PID" >/dev/null 2>&1 || true
    fi
    rm -f "$CONFIG_FILE"
    rmdir "${XDG_CONFIG_HOME}/threadplane" >/dev/null 2>&1 || true
    rmdir "$XDG_CONFIG_HOME" >/dev/null 2>&1 || true
    docker compose -p "$PROJECT" down -v >/dev/null 2>&1 || true
}

trap cleanup EXIT

mkdir -p "$(dirname "$CONFIG_FILE")"
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

[server.workspace_bootstrap.auth]
allowed_algorithms = ["ssh_ed25519"]
challenge_ttl_seconds = 90
signed_commands_required = true

[server.workspace_bootstrap.priorities]
default_priority = "medium"

[[server.workspace_bootstrap.priorities.priorities]]
name = "low"
rank = 10
description = "Useful but not urgent."

[[server.workspace_bootstrap.priorities.priorities]]
name = "medium"
rank = 20
description = "Default day-to-day work."

[[server.workspace_bootstrap.priorities.priorities]]
name = "high"
rank = 30
description = "Should be pulled forward."

[[server.workspace_bootstrap.priorities.priorities]]
name = "urgent"
rank = 40
description = "Drop other work and handle this now."

[[server.workspace_bootstrap.memberships]]
actor_id = "operator"
role = "admin"

[[server.workspace_bootstrap.memberships]]
actor_id = "agent-a"
role = "editor"

[[server.workspace_bootstrap.memberships]]
actor_id = "agent-b"
role = "editor"

[[server.workspace_bootstrap.memberships]]
actor_id = "agent-c"
role = "editor"

[[server.workspace_bootstrap.public_keys]]
actor_id = "operator"
algorithm = "ssh_ed25519"
key_id = "local"
public_key = "ssh-ed25519 AAAATEST threadplane@example"
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

initial_projection_status_json="$(
    cargo run -q -p threadplane-cli -- \
        projection status
)"
[[ "$(jq -r '.data.projection_name' <<<"$initial_projection_status_json")" == "neo4j_graph" ]]
[[ "$(jq -r '.data.caught_up' <<<"$initial_projection_status_json")" == "true" ]]
[[ "$(jq -r '.data.total_events' <<<"$initial_projection_status_json")" == "0" ]]

epic_json="$(
    cargo run -q -p threadplane-cli -- \
        epic add \
        --workspace "$WORKSPACE" \
        --author operator \
        --title "Workflow foundations" \
        --body "Dogfood first-class epics and task DAGs."
)"
epic_id="$(jq -r '.data.epic_id' <<<"$epic_json")"
epic_ref="$(jq -r '.data.entity_ref' <<<"$epic_json")"

dependency_task_json="$(
    cargo run -q -p threadplane-cli -- \
        task offer \
        --workspace "$WORKSPACE" \
        --author operator \
        --epic-id "$epic_id" \
        --owner platform \
        --label workflow \
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
        --priority high \
        --owner codex \
        --label agent \
        --label workflow \
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
[[ "$(jq -r '.data.priority' <<<"$task_show_json")" == "high" ]]
[[ "$(jq -r '.data.owner' <<<"$task_show_json")" == "codex" ]]
[[ "$(jq -r '.data.labels | join(",")' <<<"$task_show_json")" == "agent,workflow" ]]

task_list_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --status open
)"
[[ "$(jq -r '.data | length' <<<"$task_list_json")" == "2" ]]
[[ "$(jq -r --arg task_id "$task_id" '.data[] | select(.task.task_id == $task_id) | .ready' <<<"$task_list_json")" == "false" ]]
[[ "$(jq -r --arg task_id "$task_id" '.data[] | select(.task.task_id == $task_id) | .epic.epic_id' <<<"$task_list_json")" == "$epic_id" ]]
[[ "$(jq -r --arg task_id "$task_id" '.data[] | select(.task.task_id == $task_id) | .task.priority' <<<"$task_list_json")" == "high" ]]

owner_filtered_tasks_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --owner codex
)"
[[ "$(jq -r '.data | length' <<<"$owner_filtered_tasks_json")" == "1" ]]
[[ "$(jq -r '.data[0].task.task_id' <<<"$owner_filtered_tasks_json")" == "$task_id" ]]

label_filtered_tasks_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --label workflow
)"
[[ "$(jq -r '.data | length' <<<"$label_filtered_tasks_json")" == "2" ]]

priority_filtered_tasks_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --priority high
)"
[[ "$(jq -r '.data | length' <<<"$priority_filtered_tasks_json")" == "1" ]]
[[ "$(jq -r '.data[0].task.task_id' <<<"$priority_filtered_tasks_json")" == "$task_id" ]]

dag_json="$(
    cargo run -q -p threadplane-cli -- \
        task dag \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.dependencies[0].task_id' <<<"$dag_json")" == "$dependency_task_id" ]]
[[ "$(jq -r '.data.dependencies[0].depth' <<<"$dag_json")" == "1" ]]

blocked_by_compact="$(
    cargo run -q -p threadplane-cli -- \
        task blocked-by \
        --task-id "$task_id"
)"
[[ "$blocked_by_compact" == *"Ship durable task lifecycle"* ]]
[[ "$blocked_by_compact" == *"depth=1"* ]]

blocks_compact="$(
    cargo run -q -p threadplane-cli -- \
        task blocks \
        --task-id "$dependency_task_id" \
        --direct-only
)"
[[ "$blocks_compact" == *"Investigate tuple leases"* ]]
[[ "$blocks_compact" == *"depth=1"* ]]

complete_dependency_json="$(
    cargo run -q -p threadplane-cli -- \
        task complete \
        --workspace "$WORKSPACE" \
        --actor operator \
        --task-id "$dependency_task_id"
)"
[[ "$(jq -r '.data.status' <<<"$complete_dependency_json")" == "completed" ]]

low_priority_task_json="$(
    cargo run -q -p threadplane-cli -- \
        task offer \
        --workspace "$WORKSPACE" \
        --author operator \
        --priority low \
        --owner backlog \
        --label queue \
        --title "Archive stale benchmark notes" \
        --details "A lower-priority ready task used to verify queue ordering."
)"
low_priority_task_id="$(jq -r '.data.task_id' <<<"$low_priority_task_json")"

ready_tasks_json="$(
    cargo run -q -p threadplane-cli -- \
        task list \
        --workspace "$WORKSPACE" \
        --status open \
        --ready-only
)"
[[ "$(jq -r '.data | length' <<<"$ready_tasks_json")" == "2" ]]
[[ "$(jq -r '.data[0].task.task_id' <<<"$ready_tasks_json")" == "$task_id" ]]
[[ "$(jq -r '.data[1].task.task_id' <<<"$ready_tasks_json")" == "$low_priority_task_id" ]]

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
[[ "$compact_ready_queue" == *"priority=high"* ]]
[[ "$compact_ready_queue" == *"owner=codex"* ]]
[[ "$compact_ready_queue" == *"labels=agent,workflow"* ]]
[[ "$compact_ready_queue" != *"Archive stale benchmark notes"* ]]

next_task_json="$(
    cargo run -q -p threadplane-cli -- \
        task next \
        --workspace "$WORKSPACE"
)"
[[ "$(jq -r '.data.task.task_id' <<<"$next_task_json")" == "$task_id" ]]

next_task_compact="$(
    cargo run -q -p threadplane-cli -- \
        task next \
        --workspace "$WORKSPACE" \
        --format compact
)"
[[ "$next_task_compact" == *"Investigate tuple leases"* ]]
[[ "$next_task_compact" == *"priority=high"* ]]

idempotent_note_json="$(
    cargo run -q -p threadplane-cli -- \
        --idempotency-key "note-seed-${WORKSPACE}" \
        note add \
        --workspace "$WORKSPACE" \
        --author agent-c \
        --title "Idempotent note seed" \
        --body "This note should only be recorded once."
)"
replayed_idempotent_note_json="$(
    cargo run -q -p threadplane-cli -- \
        --idempotency-key "note-seed-${WORKSPACE}" \
        note add \
        --workspace "$WORKSPACE" \
        --author agent-c \
        --title "Idempotent note seed" \
        --body "This note should only be recorded once."
)"
[[ "$(jq -r '.data.note_id' <<<"$idempotent_note_json")" == "$(jq -r '.data.note_id' <<<"$replayed_idempotent_note_json")" ]]
[[ "$(jq -r '.receipt.replayed' <<<"$idempotent_note_json")" == "false" ]]
[[ "$(jq -r '.receipt.replayed' <<<"$replayed_idempotent_note_json")" == "true" ]]
[[ "$(jq -r '.receipt.idempotency_key' <<<"$replayed_idempotent_note_json")" == "note-seed-${WORKSPACE}" ]]

memory_json="$(
    cargo run -q -p threadplane-cli -- \
        memory add \
        --workspace "$WORKSPACE" \
        --author agent-c \
        --title "Core engineering memory" \
        --body "Prefer clean bottom-up abstractions and prime new sessions with durable context." \
        --kind workflow \
        --scope workspace \
        --audience both \
        --importance critical \
        --tag prime \
        --tag core \
        --recall-trigger session_start \
        --recall-trigger before_codegen
)"
memory_id="$(jq -r '.data.memory_id' <<<"$memory_json")"
memory_ref="$(jq -r '.data.entity_ref' <<<"$memory_json")"

memory_list_json="$(
    cargo run -q -p threadplane-cli -- \
        memory list \
        --workspace "$WORKSPACE" \
        --tag prime \
        --importance critical
)"
[[ "$(jq -r '.data | length' <<<"$memory_list_json")" == "1" ]]
[[ "$(jq -r '.data[0].kind' <<<"$memory_list_json")" == "workflow" ]]
[[ "$(jq -r '.data[0].audience' <<<"$memory_list_json")" == "both" ]]

memory_prime_json="$(
    cargo run -q -p threadplane-cli -- \
        memory prime \
        --workspace "$WORKSPACE"
)"
[[ "$(jq -r '.data | length' <<<"$memory_prime_json")" == "1" ]]
[[ "$(jq -r '.data[0].memory_id' <<<"$memory_prime_json")" == "$memory_id" ]]
[[ "$(jq -r '.data[0].recall_triggers | join(",")' <<<"$memory_prime_json")" == "before_codegen,session_start" ]]

memory_link_json="$(
    cargo run -q -p threadplane-cli -- \
        link add \
        --workspace "$WORKSPACE" \
        --actor operator \
        --from "$memory_ref" \
        --relation guides_task \
        --to "$task_ref"
)"
[[ "$(jq -r '.data.relation' <<<"$memory_link_json")" == "guides_task" ]]

memory_show_json="$(
    cargo run -q -p threadplane-cli -- \
        memory show \
        --memory-id "$memory_id"
)"
[[ "$(jq -r '.data.title' <<<"$memory_show_json")" == "Core engineering memory" ]]

memory_entity_show_json="$(
    cargo run -q -p threadplane-cli -- \
        entity show \
        --entity-ref "$memory_ref"
)"
[[ "$(jq -r '.data.entity.kind' <<<"$memory_entity_show_json")" == "memory" ]]
[[ "$(jq -r '.data.entity.record.memory_id' <<<"$memory_entity_show_json")" == "$memory_id" ]]
[[ "$(jq -r --arg task_ref "$task_ref" '.data.relations[] | select(.entity_ref == $task_ref) | .relation' <<<"$memory_entity_show_json")" == "GUIDES_TASK" ]]

note_json="$(
    cargo run -q -p threadplane-cli -- \
        note add \
        --workspace "$WORKSPACE" \
        --author agent-a \
        --title "Lease design note" \
        --body "Claims should expire and return tasks to the pool."
)"
note_id="$(jq -r '.data.note_id' <<<"$note_json")"
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
        --note-id "$note_id" \
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
        --priority urgent \
        --owner ops \
        --label xanadu \
        --label sync \
        --title "Canonical lease wording" \
        --details "Updates from the task side should also rewrite the linked note."
)"

note_after_task_update_json="$(
    cargo run -q -p threadplane-cli -- \
        note show \
        --note-id "$note_id"
)"
[[ "$(jq -r '.data.title' <<<"$note_after_task_update_json")" == "Canonical lease wording" ]]
[[ "$(jq -r '.data.body' <<<"$note_after_task_update_json")" == "Updates from the task side should also rewrite the linked note." ]]
[[ "$(jq -r '.data.transclusion_id' <<<"$note_after_task_update_json")" == "$transclusion_id" ]]

note_list_json="$(
    cargo run -q -p threadplane-cli -- \
        note list \
        --workspace "$WORKSPACE"
)"
[[ "$(jq -r --arg note_id "$note_id" '[.data[] | select(.note_id == $note_id)] | length' <<<"$note_list_json")" == "1" ]]

note_search_json="$(
    cargo run -q -p threadplane-cli -- \
        note search \
        --workspace "$WORKSPACE" \
        --query "Canonical"
)"
[[ "$(jq -r '.data[0].title' <<<"$note_search_json")" == "Canonical lease wording" ]]

note_search_compact="$(
    cargo run -q -p threadplane-cli -- \
        note search \
        --workspace "$WORKSPACE" \
        --query "lease" \
        --format compact
)"
[[ "$note_search_compact" == *"Canonical lease wording"* ]]

entity_show_json="$(
    cargo run -q -p threadplane-cli -- \
        entity show \
        --entity-ref "$note_ref"
)"
[[ "$(jq -r '.data.entity.kind' <<<"$entity_show_json")" == "note" ]]
[[ "$(jq -r '.data.entity.record.note_id' <<<"$entity_show_json")" == "$note_id" ]]
[[ "$(jq -r --arg task_ref "$task_ref" '.data.relations[] | select(.entity_ref == $task_ref) | .relation' <<<"$entity_show_json")" == "XANADU_LINK" ]]

epic_related_json="$(
    cargo run -q -p threadplane-cli -- \
        entity related \
        --entity-ref "$epic_ref"
)"
[[ "$(jq -r --arg task_ref "$task_ref" '.data[] | select(.entity_ref == $task_ref) | .relation' <<<"$epic_related_json")" == "IMPLEMENTS_EPIC" ]]

entity_related_compact="$(
    cargo run -q -p threadplane-cli -- \
        entity related \
        --entity-ref "$note_ref" \
        --format compact
)"
[[ "$entity_related_compact" == *"XANADU_LINK"* ]]
[[ "$entity_related_compact" == *"${task_ref%%-*}"* ]]

updated_task_show_json="$(
    cargo run -q -p threadplane-cli -- \
        task show \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.priority' <<<"$updated_task_show_json")" == "urgent" ]]
[[ "$(jq -r '.data.owner' <<<"$updated_task_show_json")" == "ops" ]]
[[ "$(jq -r '.data.labels | join(",")' <<<"$updated_task_show_json")" == "sync,xanadu" ]]

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
        --priority low \
        --owner backlog \
        --label triaged \
        --complete \
        --task-id "$triage_task_a_id" \
        --task-id "$triage_task_b_id"
)"
[[ "$(jq -r '.completed_task_ids | length' <<<"$triage_json")" == "2" ]]
[[ "$(jq -r '.updated_task_ids | length' <<<"$triage_json")" == "2" ]]
[[ "$(jq -r '.unchanged_task_ids | length' <<<"$triage_json")" == "0" ]]
[[ "$(jq -r '.priority' <<<"$triage_json")" == "low" ]]
[[ "$(jq -r '.owner' <<<"$triage_json")" == "backlog" ]]
[[ "$(jq -r '.labels | join(",")' <<<"$triage_json")" == "triaged" ]]

triage_task_a_show_json="$(
    cargo run -q -p threadplane-cli -- \
        task show \
        --task-id "$triage_task_a_id"
)"
[[ "$(jq -r '.data.priority' <<<"$triage_task_a_show_json")" == "low" ]]
[[ "$(jq -r '.data.owner' <<<"$triage_task_a_show_json")" == "backlog" ]]
[[ "$(jq -r '.data.labels | join(",")' <<<"$triage_task_a_show_json")" == "triaged" ]]

context_before_claim_json="$(
    cargo run -q -p threadplane-cli -- \
        task context \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.ready' <<<"$context_before_claim_json")" == "true" ]]
[[ "$(jq -r '.data.epic.epic_id' <<<"$context_before_claim_json")" == "$epic_id" ]]
[[ "$(jq -r '.data.dependencies[0].task_id' <<<"$context_before_claim_json")" == "$dependency_task_id" ]]

claim_next_json="$(
    cargo run -q -p threadplane-cli -- \
        task claim-next \
        --workspace "$WORKSPACE" \
        --actor agent-b \
        --priority urgent \
        --lease-seconds 120
)"
[[ "$(jq -r '.data.task_id' <<<"$claim_next_json")" == "$task_id" ]]

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

tail_events_json="$(
    cargo run -q -p threadplane-cli -- \
        events tail \
        --workspace "$WORKSPACE" \
        --after-event-id "$(jq -r '.data.event_id' <<<"$claim_next_json")" \
        --limit 5
)"
[[ "$(jq -r '.data[0].kind' <<<"$tail_events_json")" == "task_released" ]]
[[ "$(jq -r '.data[1].kind' <<<"$tail_events_json")" == "task_claimed" ]]
[[ "$(jq -r '.data[2].kind' <<<"$tail_events_json")" == "task_completed" ]]

tail_events_compact="$(
    cargo run -q -p threadplane-cli -- \
        events tail \
        --workspace "$WORKSPACE" \
        --after-event-id "$(jq -r '.data.event_id' <<<"$claim_next_json")" \
        --limit 5 \
        --format compact
)"
[[ "$tail_events_compact" == *"task_released"* ]]
[[ "$tail_events_compact" == *"task_claimed"* ]]
[[ "$tail_events_compact" == *"task_completed"* ]]

events_json="$(
    cargo run -q -p threadplane-cli -- \
        events list \
        --workspace "$WORKSPACE" \
        --limit 40
)"
[[ "$(jq -r '.data[0].kind' <<<"$events_json")" == "task_completed" ]]
[[ "$(jq -r '.data[] | select(.kind == "task_dependency_declared") | .kind' <<<"$events_json" | head -n1)" == "task_dependency_declared" ]]
[[ "$(jq -r '.data[] | select(.kind == "epic_recorded") | .kind' <<<"$events_json" | head -n1)" == "epic_recorded" ]]
[[ "$(jq '[.data[] | select(.kind == "note_recorded")] | length' <<<"$events_json")" == "2" ]]

for _attempt in $(seq 1 20); do
    projection_status_json="$(
        cargo run -q -p threadplane-cli -- \
            projection status
    )"
    if [[ "$(jq -r '.data.caught_up' <<<"$projection_status_json")" == "true" ]] \
        && [[ "$(jq -r '.data.total_events' <<<"$projection_status_json")" != "0" ]]; then
        break
    fi
    sleep 1
done
[[ "$(jq -r '.data.projection_name' <<<"$projection_status_json")" == "neo4j_graph" ]]
[[ "$(jq -r '.data.caught_up' <<<"$projection_status_json")" == "true" ]]
[[ "$(jq -r '.data.total_events' <<<"$projection_status_json")" != "0" ]]
[[ "$(jq -r '.data.pending_events' <<<"$projection_status_json")" == "0" ]]

scope_json="$(
    cargo run -q -p threadplane-cli -- \
        scope
)"
[[ "$(jq -r '.projection.projection_name' <<<"$scope_json")" == "neo4j_graph" ]]
[[ "$(jq -r '.projection.caught_up' <<<"$scope_json")" == "true" ]]
[[ "$(jq -r '.projection.total_events' <<<"$scope_json")" != "0" ]]

echo "threadplane e2e ok"
echo "workspace=$WORKSPACE"
echo "$epic_json"
echo "$dependency_task_json"
echo "$task_json"
echo "$note_json"
echo "$link_json"
echo "$note_update_json"
echo "$task_update_json"
echo "$claim_next_json"
echo "$released_task_json"
echo "$completed_task_json"
