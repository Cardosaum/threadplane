#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PROJECT="threadplane-demo-$(date +%s)"
WORKSPACE="demo-$(date +%s)"
XDG_CONFIG_HOME="$(mktemp -d)"
CONFIG_FILE="${XDG_CONFIG_HOME}/threadplane/config.toml"
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
export XDG_CONFIG_HOME

cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
        kill "$SERVER_PID" >/dev/null 2>&1 || true
        wait "$SERVER_PID" >/dev/null 2>&1 || true
    fi
    rm -f "$CONFIG_FILE"
    rmdir "${XDG_CONFIG_HOME}/threadplane" >/dev/null 2>&1 || true
    rmdir "$XDG_CONFIG_HOME" >/dev/null 2>&1 || true
    rm -f "$SERVER_LOG"
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

cargo build -q -p threadplane-cli -p threadplane-server
./target/debug/threadplane-server >"$SERVER_LOG" 2>&1 &
SERVER_PID="$!"

until curl -sf "http://127.0.0.1:${server_port}/healthz" >/dev/null; do
    if ! kill -0 "$SERVER_PID" >/dev/null 2>&1; then
        cat "$SERVER_LOG"
        exit 1
    fi
    sleep 1
done

THREADPLANE_DEMO_WORKSPACE="$WORKSPACE" \
DEMO_COMMAND_PAUSE_SECONDS="3.2" \
DEMO_RESULT_PAUSE_SECONDS="4.4" \
DEMO_INTRO_PAUSE_SECONDS="2.0" \
TP_BIN="./target/debug/tplane" \
asciinema record \
    --overwrite \
    --headless \
    --title "threadplane quick tour" \
    --idle-time-limit 4.5 \
    --window-size 100x26 \
    --command "./scripts/demo-session.sh" \
    docs/threadplane-demo.cast

python3 - <<'PY'
from pathlib import Path
import json

cast_path = Path("docs/threadplane-demo.cast")
lines = cast_path.read_text().splitlines()
header = lines[0]
events = [json.loads(line) for line in lines[1:]]
if events:
    events[0][0] = 0.05
cast_path.write_text("\n".join([header, *[json.dumps(event, separators=(",", ":")) for event in events]]) + "\n")
PY

agg \
    --theme github-dark \
    --font-family "JetBrains Mono,Fira Code,DejaVu Sans Mono" \
    --font-size 16 \
    --fps-cap 20 \
    --speed 0.65 \
    --idle-time-limit 4.5 \
    --last-frame-duration 3.0 \
    --cols 96 \
    --rows 24 \
    docs/threadplane-demo.cast \
    docs/threadplane-demo.gif
