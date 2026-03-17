# Configuration

`threadplane` uses TOML for application config and environment variables for overrides and local infrastructure wiring.
It does not ship built-in runtime defaults, so every value must be stated explicitly in config, env, or CLI overrides.

## Discovery Order

The CLI and server layer config in this order:

1. `--config /path/to/config.toml` for explicit one-off CLI runs
2. `THREADPLANE_CONFIG=/path/to/config.toml`
3. XDG user config at `${XDG_CONFIG_HOME:-$HOME/.config}/threadplane/config.toml`
4. XDG system config directories such as `/etc/xdg/threadplane/config.toml`
5. `THREADPLANE__...` nested environment overrides
6. CLI runtime overrides such as `--server`

Inspect the resolved config and discovery order with:

```bash
cargo run -p threadplane-cli -- config show
```

## Local Files

`./scripts/generate-env.sh` creates two local files:

- `.env`
  Used by Docker Compose for PostgreSQL and Neo4j credentials.
- `${XDG_CONFIG_HOME:-$HOME/.config}/threadplane/config.toml`
  Used by `threadplane-server` and `threadplane-cli`.

The generated config lives in your XDG config home, not in the repo worktree.

## Config Shape

Example:

```toml
[cli]
url = "http://127.0.0.1:4000"

[server]
bind = "127.0.0.1:4000"
database_url = "postgres://threadplane:password@127.0.0.1:5432/threadplane"
default_lease_seconds = 300
neo4j_password = "password"
neo4j_uri = "127.0.0.1:7687"
neo4j_user = "neo4j"
```

See [../etc/config.toml.example](../etc/config.toml.example) for the committed example shape.

## One-Off Overrides

Use a specific config file:

```bash
cargo run -p threadplane-cli -- --config /path/to/config.toml scope
```

Override just the server URL:

```bash
cargo run -p threadplane-cli -- --server http://127.0.0.1:4010 scope
```

That flag is now applied through the same Figment pipeline as file and env config, so the resolved `config show` output reflects the final layered value instead of a post-load ad hoc override.

Override a nested value through the environment:

```bash
THREADPLANE__CLI__URL=http://127.0.0.1:4010 \
cargo run -p threadplane-cli -- scope
```

## Which Settings Matter Most

`cli.url`
- Base URL used by the CLI when `--server` is not provided.

`server.bind`
- Socket address used by `threadplane-server`.

`server.database_url`
- PostgreSQL connection string for the append-only event log and task state.

`server.neo4j_uri`
- Neo4j Bolt address used for graph projection and traversal.

`server.neo4j_user`
- Neo4j username.

`server.neo4j_password`
- Neo4j password.

`server.default_lease_seconds`
- Default lease duration applied when task claims omit `--lease-seconds`.

## Troubleshooting

If the CLI cannot connect:

- run `cargo run -p threadplane-cli -- config show`
- confirm `cli.url` points at the running server
- confirm the server is listening on `server.bind`

If the server fails to boot:

- confirm PostgreSQL is reachable through `server.database_url`
- confirm Neo4j is reachable through `server.neo4j_uri`
- make sure local Docker services are up with `docker compose ps`

If you want a clean local reset:

- stop local services with `docker compose down -v`
- rerun `./scripts/generate-env.sh --force`
- start again with `docker compose up -d`
