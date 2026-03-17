#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT_DIR}/.env"
CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
CONFIG_DIR="${CONFIG_HOME}/threadplane"
CONFIG_FILE="${CONFIG_DIR}/config.toml"

if [[ ( -f "$ENV_FILE" || -f "$CONFIG_FILE" ) && "${1:-}" != "--force" ]]; then
    echo "local config already exists"
    echo "  env: $ENV_FILE"
    echo "  app config: $CONFIG_FILE"
    echo "Run scripts/generate-env.sh --force to overwrite them."
    exit 1
fi

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing required command: $1" >&2
        exit 1
    fi
}

require_cmd openssl

mkdir -p "$CONFIG_DIR"

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
EOF

cat >"$CONFIG_FILE" <<EOF
[cli]
url = "http://127.0.0.1:4000"

[server]
bind = "127.0.0.1:4000"
database_url = "postgres://threadplane:${postgres_password}@127.0.0.1:5432/threadplane"
default_lease_seconds = 300
neo4j_password = "${neo4j_password}"
neo4j_uri = "127.0.0.1:7687"
neo4j_user = "neo4j"
EOF

echo "generated $ENV_FILE"
echo "generated $CONFIG_FILE"
