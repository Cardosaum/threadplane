#!/usr/bin/env bash
set -euo pipefail

TP_BIN="${TP_BIN:-./target/debug/threadplane-cli}"
WORKSPACE="${THREADPLANE_DEMO_WORKSPACE:-demo-lab}"

run() {
    local display="$1"
    local command="$2"
    printf '\033[1;32m$\033[0m %s\n' "$display"
    eval "$command"
    printf '\n'
    sleep 0.6
}

epic_json="$("$TP_BIN" epic add \
    --workspace "$WORKSPACE" \
    --author operator \
    --title "Dogfood threadplane" \
    --body "Use threadplane itself to coordinate repo improvement work.")"
epic_id="$(jq -r '.data.epic_id' <<<"$epic_json")"

task_a_json="$("$TP_BIN" task offer \
    --workspace "$WORKSPACE" \
    --author operator \
    --epic-id "$epic_id" \
    --title "Harden the write path" \
    --details "Ship auth, idempotency, and replay-safe projections.")"
task_a_id="$(jq -r '.data.task_id' <<<"$task_a_json")"

task_b_json="$("$TP_BIN" task offer \
    --workspace "$WORKSPACE" \
    --author operator \
    --epic-id "$epic_id" \
    --depends-on "$task_a_id" \
    --title "Benchmark concurrency" \
    --details "Measure mixed read and write load before broad rollout.")"
task_b_id="$(jq -r '.data.task_id' <<<"$task_b_json")"
task_b_ref="$(jq -r '.data.entity_ref' <<<"$task_b_json")"

note_json="$("$TP_BIN" note add \
    --workspace "$WORKSPACE" \
    --author agent-a \
    --title "Benchmark note" \
    --body "Stress tests should track projection lag and replay health.")"
note_id="$(jq -r '.data.note_id' <<<"$note_json")"
note_ref="$(jq -r '.data.entity_ref' <<<"$note_json")"

sleep 0.8

printf 'threadplane quick tour\n'
printf 'shared memory, task DAGs, and xanadu links for agents\n\n'
sleep 1

run 'threadplane-cli scope | jq "{name, log: .poc.authoritative_log, graph: .poc.graph_projection}"' \
    "\"$TP_BIN\" scope | jq '{name, log: .poc.authoritative_log, graph: .poc.graph_projection}'"
run "threadplane-cli epic list --workspace \"$WORKSPACE\" | jq '.data[] | {epic_id, title}'" \
    "\"$TP_BIN\" epic list --workspace \"$WORKSPACE\" | jq '.data[] | {epic_id, title}'"
run "threadplane-cli task list --workspace \"$WORKSPACE\" --status open | jq '.data[] | {title: .task.title, ready, blocked_by: .dependencies}'" \
    "\"$TP_BIN\" task list --workspace \"$WORKSPACE\" --status open | jq '.data[] | {title: .task.title, ready, blocked_by: .dependencies}'"
run "threadplane-cli task complete --workspace \"$WORKSPACE\" --actor operator --task-id \"$task_a_id\" | jq '{task_id: .data.task_id, status: .data.status}'" \
    "\"$TP_BIN\" task complete --workspace \"$WORKSPACE\" --actor operator --task-id \"$task_a_id\" | jq '{task_id: .data.task_id, status: .data.status}'"
run "threadplane-cli task list --workspace \"$WORKSPACE\" --status open --ready-only | jq '.data[] | {title: .task.title, ready}'" \
    "\"$TP_BIN\" task list --workspace \"$WORKSPACE\" --status open --ready-only | jq '.data[] | {title: .task.title, ready}'"
run "threadplane-cli link xanadu --workspace \"$WORKSPACE\" --actor agent-a --from \"$task_b_ref\" --to \"$note_ref\" | jq '{relation: .data.relation, transclusion_id: .data.transclusion_id}'" \
    "\"$TP_BIN\" link xanadu --workspace \"$WORKSPACE\" --actor agent-a --from \"$task_b_ref\" --to \"$note_ref\" | jq '{relation: .data.relation, transclusion_id: .data.transclusion_id}'"
run "threadplane-cli note update --workspace \"$WORKSPACE\" --actor agent-a --note-id \"$note_id\" ... | jq '{title: .data.title, transclusion_id: .data.transclusion_id}'" \
    "\"$TP_BIN\" note update --workspace \"$WORKSPACE\" --actor agent-a --note-id \"$note_id\" --title 'Concurrency benchmark plan' --body 'Track latency, throughput, projection lag, and claim contention.' | jq '{title: .data.title, transclusion_id: .data.transclusion_id}'"
run "threadplane-cli task context --task-id \"$task_b_id\" | jq '{task: .data.task.title, details: .data.task.details, relations: .data.relations}'" \
    "\"$TP_BIN\" task context --task-id \"$task_b_id\" | jq '{task: .data.task.title, details: .data.task.details, relations: .data.relations}'"
