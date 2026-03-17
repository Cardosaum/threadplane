#!/usr/bin/env bash
set -euo pipefail

TP_BIN="${TP_BIN:-./target/debug/threadplane-cli}"
WORKSPACE="${THREADPLANE_DEMO_WORKSPACE:-demo-lab}"
DEMO_COMMAND_PAUSE_SECONDS="${DEMO_COMMAND_PAUSE_SECONDS:-1.8}"
DEMO_RESULT_PAUSE_SECONDS="${DEMO_RESULT_PAUSE_SECONDS:-2.4}"
DEMO_INTRO_PAUSE_SECONDS="${DEMO_INTRO_PAUSE_SECONDS:-1.2}"

run() {
    local display="$1"
    local command="$2"
    printf '\033[1;32m$\033[0m %s\n' "$display"
    sleep "$DEMO_COMMAND_PAUSE_SECONDS"
    eval "$command"
    printf '\n'
    sleep "$DEMO_RESULT_PAUSE_SECONDS"
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
    --priority high \
    --owner platform \
    --label hardening \
    --title "Harden the write path" \
    --details "Ship auth, idempotency, and replay-safe projections.")"
task_a_id="$(jq -r '.data.task_id' <<<"$task_a_json")"

task_b_json="$("$TP_BIN" task offer \
    --workspace "$WORKSPACE" \
    --author operator \
    --epic-id "$epic_id" \
    --depends-on "$task_a_id" \
    --priority urgent \
    --owner codex \
    --label agent \
    --label benchmark \
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

sleep "$DEMO_INTRO_PAUSE_SECONDS"

run 'threadplane-cli scope 2>/dev/null | jq "{name, log: .poc.authoritative_log, graph: .poc.graph_projection}"' \
    "\"$TP_BIN\" scope 2>/dev/null | jq '{name, log: .poc.authoritative_log, graph: .poc.graph_projection}'"
run "threadplane-cli epic list --workspace \"$WORKSPACE\" | jq '.data[] | {title}'" \
    "\"$TP_BIN\" epic list --workspace \"$WORKSPACE\" | jq '.data[] | {title}'"
run "threadplane-cli task list --workspace \"$WORKSPACE\" --status open --format compact" \
    "\"$TP_BIN\" task list --workspace \"$WORKSPACE\" --status open --format compact"
run "threadplane-cli task blocked-by --task-id \"$task_b_id\"" \
    "\"$TP_BIN\" task blocked-by --task-id \"$task_b_id\""
run "threadplane-cli task blocks --task-id \"$task_a_id\" --direct-only" \
    "\"$TP_BIN\" task blocks --task-id \"$task_a_id\" --direct-only"
run "threadplane-cli task complete --workspace \"$WORKSPACE\" --actor operator --task-id \"$task_a_id\" | jq '{task_id: .data.task_id, status: .data.status}'" \
    "\"$TP_BIN\" task complete --workspace \"$WORKSPACE\" --actor operator --task-id \"$task_a_id\" | jq '{task_id: .data.task_id, status: .data.status}'"
run "threadplane-cli task list --workspace \"$WORKSPACE\" --status open --ready-only --format compact" \
    "\"$TP_BIN\" task list --workspace \"$WORKSPACE\" --status open --ready-only --format compact"
run "threadplane-cli link xanadu --workspace \"$WORKSPACE\" --actor agent-a --from \"$task_b_ref\" --to \"$note_ref\" | jq '{relation: .data.relation, transclusion_id: .data.transclusion_id}'" \
    "\"$TP_BIN\" link xanadu --workspace \"$WORKSPACE\" --actor agent-a --from \"$task_b_ref\" --to \"$note_ref\" | jq '{relation: .data.relation, transclusion_id: .data.transclusion_id}'"
run "threadplane-cli note update --workspace \"$WORKSPACE\" --actor agent-a --note-id \"$note_id\" ... | jq '{title: .data.title, transclusion_id: .data.transclusion_id}'" \
    "\"$TP_BIN\" note update --workspace \"$WORKSPACE\" --actor agent-a --note-id \"$note_id\" --title 'Concurrency benchmark plan' --body 'Track latency, throughput, projection lag, and claim contention.' | jq '{title: .data.title, transclusion_id: .data.transclusion_id}'"
run "threadplane-cli task show --task-id \"$task_b_id\" | jq '{title: .data.title, priority: .data.priority, owner: .data.owner, labels: .data.labels, transclusion_id: .data.transclusion_id}'" \
    "\"$TP_BIN\" task show --task-id \"$task_b_id\" | jq '{title: .data.title, priority: .data.priority, owner: .data.owner, labels: .data.labels, transclusion_id: .data.transclusion_id}'"
