#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_FILE="${ROOT_DIR}/etc/config.toml"
ENV_FILE="${ROOT_DIR}/.env"
RUNTIME_DIR="${ROOT_DIR}/.local/threadplane"
PID_FILE="${RUNTIME_DIR}/server.pid"
LOG_FILE="${RUNTIME_DIR}/server.log"
COMMAND="${1:-up}"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing required command: $1" >&2
        exit 1
    fi
}

ensure_runtime_dir() {
    mkdir -p "$RUNTIME_DIR"
}

ensure_local_config() {
    if [[ -f "$CONFIG_FILE" && -f "$ENV_FILE" ]]; then
        return
    fi

    echo "local config not found; generating fresh local credentials"
    "${ROOT_DIR}/scripts/generate-env.sh"
}

config_value() {
    local path="$1"
    python3 - "$CONFIG_FILE" "$path" <<'PY'
import sys, tomllib
from pathlib import Path

config_path = Path(sys.argv[1])
keys = sys.argv[2].split(".")
value = tomllib.loads(config_path.read_text())
for key in keys:
    value = value[key]
print(value)
PY
}

server_url() {
    config_value "cli.url"
}

server_bind() {
    config_value "server.bind"
}

server_port() {
    python3 - "$(server_bind)" <<'PY'
import sys
bind = sys.argv[1]
host, port = bind.rsplit(":", 1)
print(port)
PY
}

load_env_file() {
    set -a
    source "$ENV_FILE"
    set +a
}

pretty_json() {
    if command -v jq >/dev/null 2>&1; then
        jq
    else
        cat
    fi
}

is_pid_running() {
    local pid="$1"
    kill -0 "$pid" >/dev/null 2>&1
}

stop_managed_server() {
    if [[ -f "$PID_FILE" ]]; then
        local pid
        pid="$(<"$PID_FILE")"
        if [[ -n "$pid" ]] && is_pid_running "$pid"; then
            kill "$pid" >/dev/null 2>&1 || true
            wait "$pid" >/dev/null 2>&1 || true
        fi
        rm -f "$PID_FILE"
    fi
}

stop_stale_server_on_port() {
    local port
    port="$(server_port)"
    if ! command -v lsof >/dev/null 2>&1; then
        return
    fi

    local pids
    pids="$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -z "$pids" ]]; then
        return
    fi

    while IFS= read -r pid; do
        [[ -z "$pid" ]] && continue
        local command_line
        command_line="$(ps -p "$pid" -o args= 2>/dev/null || true)"
        if [[ "$command_line" == *threadplane-server* ]]; then
            kill "$pid" >/dev/null 2>&1 || true
        fi
    done <<<"$pids"
}

start_dependencies() {
    load_env_file
    docker compose up -d postgres neo4j >/dev/null

    until docker compose exec -T postgres pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; do
        sleep 1
    done

    until curl -sf "http://127.0.0.1:${NEO4J_HTTP_PORT}" >/dev/null; do
        sleep 1
    done
}

start_server() {
    ensure_runtime_dir
    cargo build -q -p threadplane-cli -p threadplane-server
    stop_managed_server
    stop_stale_server_on_port

    python3 - "$ROOT_DIR" "$LOG_FILE" "$PID_FILE" <<'PY'
import subprocess
import sys
from pathlib import Path

root_dir = Path(sys.argv[1])
log_path = Path(sys.argv[2])
pid_path = Path(sys.argv[3])
binary = root_dir / "target" / "debug" / "threadplane-server"

with log_path.open("wb") as log_file:
    process = subprocess.Popen(
        [str(binary)],
        cwd=root_dir,
        stdin=subprocess.DEVNULL,
        stdout=log_file,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

pid_path.write_text(f"{process.pid}\n")
PY
}

wait_for_server() {
    local url
    url="$(server_url)"
    local pid
    pid="$(<"$PID_FILE")"

    until curl -sf "${url}/healthz" >/dev/null; do
        if ! is_pid_running "$pid"; then
            cat "$LOG_FILE"
            exit 1
        fi
        sleep 1
    done
}

show_status() {
    local url
    url="$(server_url)"

    if curl -sf "${url}/healthz" >/dev/null; then
        echo "threadplane dogfood stack is ready"
        curl -sf "${url}/healthz" | pretty_json
    else
        echo "threadplane server is not healthy at ${url}" >&2
        exit 1
    fi
}

command_up() {
    require_cmd cargo
    require_cmd curl
    require_cmd docker
    require_cmd python3
    ensure_local_config
    start_dependencies
    start_server
    wait_for_server
    show_status
}

command_status() {
    require_cmd curl
    require_cmd python3
    ensure_local_config
    show_status
}

command_stop() {
    stop_managed_server
    echo "stopped managed threadplane-server"
}

case "$COMMAND" in
    up)
        command_up
        ;;
    status)
        command_status
        ;;
    stop)
        command_stop
        ;;
    *)
        echo "usage: $0 [up|status|stop]" >&2
        exit 1
        ;;
esac
