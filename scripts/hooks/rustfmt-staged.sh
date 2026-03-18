#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

declare -a rust_files=()
for candidate in "$@"; do
    if [[ "$candidate" == *.rs ]]; then
        rust_files+=("${ROOT_DIR}/${candidate}")
    fi
done

if [[ "${#rust_files[@]}" -eq 0 ]]; then
    exit 0
fi

rustfmt --edition 2021 "${rust_files[@]}"
