#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUTPUT_ROOT="${1:-benchmarks/baselines/local-release}"
TIMESTAMP="$(date -u +%Y-%m-%dT%H-%M-%SZ)"
OUTPUT_DIR="${OUTPUT_ROOT}/${TIMESTAMP}"
NOTE_WRITES_OPERATIONS="${NOTE_WRITES_OPERATIONS:-100}"
MIXED_OPERATIONS="${MIXED_OPERATIONS:-100}"
CONCURRENCY="${CONCURRENCY:-8}"

mkdir -p "$OUTPUT_DIR"

run_profile() {
    local scenario="$1"
    local operations="$2"
    local workspace="baseline-${scenario}-${TIMESTAMP,,}"
    cargo run -q -p threadplane-bench --release -- \
        run \
        --workspace "$workspace" \
        --scenario "$scenario" \
        --operations "$operations" \
        --concurrency "$CONCURRENCY" \
        --actor-prefix baseline \
        > "${OUTPUT_DIR}/${scenario}.json"
}

run_profile note-writes "$NOTE_WRITES_OPERATIONS"
run_profile mixed "$MIXED_OPERATIONS"

cat > "${OUTPUT_DIR}/README.txt" <<EOF
threadplane local-release baseline capture
captured_at=${TIMESTAMP}
concurrency=${CONCURRENCY}
note_writes_operations=${NOTE_WRITES_OPERATIONS}
mixed_operations=${MIXED_OPERATIONS}

These results are environment-specific local release baselines.
Use them as a regression reference for this machine/setup, not as a universal performance target.
EOF

printf '%s\n' "$OUTPUT_DIR"
