#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SCENARIO="${1:-mixed}"
OPERATIONS="${OPERATIONS:-100}"
CONCURRENCY="${CONCURRENCY:-8}"
WORKSPACE="${WORKSPACE:-bench-$(date +%Y%m%d-%H%M%S)}"
ACTOR_PREFIX="${ACTOR_PREFIX:-bench}"

cargo run -q -p threadplane-bench --release -- \
    run \
    --workspace "$WORKSPACE" \
    --scenario "$SCENARIO" \
    --operations "$OPERATIONS" \
    --concurrency "$CONCURRENCY" \
    --actor-prefix "$ACTOR_PREFIX"
