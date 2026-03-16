#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT_DIR}/.env"

if [[ -f "$ENV_FILE" && "${1:-}" != "--force" ]]; then
    echo ".env already exists at $ENV_FILE"
    echo "Run scripts/generate-env.sh --force to overwrite it."
    exit 1
fi

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing required command: $1" >&2
        exit 1
    fi
}

require_cmd openssl

postgres_password="$(openssl rand -hex 18)"
neo4j_password="$(openssl rand -hex 18)"

cat >"$ENV_FILE" <<EOF
POSTGRES_DB=threadplane
POSTGRES_USER=threadplane
POSTGRES_PASSWORD=${postgres_password}
POSTGRES_PORT=5432

NEO4J_USER=neo4j
NEO4J_PASSWORD=${neo4j_password}
NEO4J_HTTP_PORT=7474
NEO4J_BOLT_PORT=7687

THREADPLANE_BIND=127.0.0.1:4000
THREADPLANE_URL=http://127.0.0.1:4000
THREADPLANE_DATABASE_URL=postgres://threadplane:${postgres_password}@127.0.0.1:5432/threadplane
THREADPLANE_NEO4J_URI=127.0.0.1:7687
THREADPLANE_NEO4J_USER=neo4j
THREADPLANE_NEO4J_PASSWORD=${neo4j_password}
THREADPLANE_DEFAULT_LEASE_SECONDS=300
EOF

echo "generated $ENV_FILE"
