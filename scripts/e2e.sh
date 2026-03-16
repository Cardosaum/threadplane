#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WORKSPACE="e2e-$(date +%s)"
PROJECT="threadplane-${WORKSPACE}"
SERVER_LOG="$(mktemp)"
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
export THREADPLANE_DATABASE_URL="postgres://threadplane:${postgres_password}@127.0.0.1:${postgres_port}/threadplane"
export THREADPLANE_NEO4J_URI="127.0.0.1:${neo4j_bolt_port}"
export THREADPLANE_NEO4J_USER="neo4j"
export THREADPLANE_NEO4J_PASSWORD="${neo4j_password}"
export THREADPLANE_BIND="127.0.0.1:${server_port}"
export THREADPLANE_URL="http://127.0.0.1:${server_port}"

cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
        kill "$SERVER_PID" >/dev/null 2>&1 || true
        wait "$SERVER_PID" >/dev/null 2>&1 || true
    fi
    docker compose -p "$PROJECT" down -v >/dev/null 2>&1 || true
}

trap cleanup EXIT

docker compose -p "$PROJECT" up -d postgres neo4j >/dev/null

until docker compose -p "$PROJECT" exec -T postgres pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; do
    sleep 1
done

until curl -sf "http://127.0.0.1:${NEO4J_HTTP_PORT}" >/dev/null; do
    sleep 1
done

cargo run -q -p threadplane-server >"$SERVER_LOG" 2>&1 &
SERVER_PID="$!"

until curl -sf "$THREADPLANE_URL/healthz" >/dev/null; do
    if ! kill -0 "$SERVER_PID" >/dev/null 2>&1; then
        cat "$SERVER_LOG"
        exit 1
    fi
    sleep 1
done

task_json="$(
    cargo run -q -p threadplane-cli -- \
        task offer \
        --workspace "$WORKSPACE" \
        --author operator \
        --title "Investigate tuple leases" \
        --details "Need a shared lease-backed claim flow."
)"
task_id="$(jq -r '.data.task_id' <<<"$task_json")"
task_ref="$(jq -r '.data.entity_ref' <<<"$task_json")"

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
        link add \
        --workspace "$WORKSPACE" \
        --actor agent-a \
        --from "$task_ref" \
        --to "$note_ref" \
        --relation related_to
)"

open_before_claim_json="$(
    cargo run -q -p threadplane-cli -- \
        task list-open \
        --workspace "$WORKSPACE"
)"
[[ "$(jq -r '.data | length' <<<"$open_before_claim_json")" == "1" ]]

claim_json="$(
    cargo run -q -p threadplane-cli -- \
        task claim \
        --workspace "$WORKSPACE" \
        --actor agent-b \
        --task-id "$task_id" \
        --lease-seconds 120
)"

open_after_claim_json="$(
    cargo run -q -p threadplane-cli -- \
        task list-open \
        --workspace "$WORKSPACE"
)"
[[ "$(jq -r '.data | length' <<<"$open_after_claim_json")" == "0" ]]

context_json="$(
    cargo run -q -p threadplane-cli -- \
        task context \
        --task-id "$task_id"
)"
[[ "$(jq -r '.data.active_claim.actor' <<<"$context_json")" == "agent-b" ]]
[[ "$(jq -r '.data.relations[0].entity_ref' <<<"$context_json")" == "$note_ref" ]]
[[ "$(jq -r '.data.relations[0].relation' <<<"$context_json")" == "RELATED_TO" ]]

events_json="$(
    cargo run -q -p threadplane-cli -- \
        events list \
        --workspace "$WORKSPACE" \
        --limit 10
)"
[[ "$(jq -r '.data | length' <<<"$events_json")" == "4" ]]
[[ "$(jq -r '.data[0].kind' <<<"$events_json")" == "task_claimed" ]]
[[ "$(jq -r '.data[3].kind' <<<"$events_json")" == "task_offered" ]]

echo "threadplane e2e ok"
echo "workspace=$WORKSPACE"
echo "$task_json"
echo "$note_json"
echo "$link_json"
echo "$claim_json"
