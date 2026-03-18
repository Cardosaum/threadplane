set shell := ["bash", "-cu"]

env:
    ./scripts/generate-env.sh

server:
    cargo run -p threadplane-server

scope:
    cargo run -p threadplane-cli -- scope

cli *args:
    cargo run -p threadplane-cli -- {{args}}

check:
    cargo check --workspace

fmt:
    cargo fmt --all

e2e:
    ./scripts/e2e.sh

hooks-install:
    lefthook install

hooks-pre-commit:
    lefthook run pre-commit --all-files

hooks-pre-push:
    lefthook run pre-push
