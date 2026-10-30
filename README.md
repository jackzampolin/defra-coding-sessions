# coding-data

Parse AI coding-agent session logs into a shared [DefraDB](https://defradb.io)
GraphQL store.

`coding-data` reads the on-disk session history that Claude Code, Codex, Gemini
CLI and Grok leave behind, normalizes it into a common shape (projects,
sessions, messages, tool actions, token usage), and writes it through a
DefraDB GraphQL endpoint. It can ingest a whole history once (`backfill`), keep
following the logs as they grow (`watch`), or re-ingest sessions whose stored
message count does not match the source (`repair`).

Repository: <https://github.com/jackzampolin/defra-coding-sessions>

## Supported sources

| `--source` | Reads from (default) | Notes |
| --- | --- | --- |
| `claude` | `~/.claude/projects/**/*.jsonl` | Claude Code session transcripts |
| `codex` | `~/.codex/sessions/**/*.jsonl`, `~/.codex/archived_sessions/**/*.jsonl` | Also reconciles titles/archived flags from `~/.codex/state_5.sqlite` |
| `gemini` | `~/.gemini/tmp/*/chats/*.json`, `~/.gemini/tmp/*/logs.json` | Project roots resolved from `.project_root` / `projects.json` |
| `grok` | `~/.grok/sessions/<url-encoded-cwd>/<session-id>/chat_history.jsonl` | Also reads `updates.jsonl` usage journals and `subagents/*/meta.json` |

Override the root with `--source-dir` / `CODING_DATA_SOURCE_DIR` /
`source_dir` in the config file. For Codex, `--sqlite-path` /
`CODING_DATA_SQLITE_PATH` / `sqlite_path` points at the catalog database
(default `~/.codex/state_5.sqlite`).

## The store it expects

Any DefraDB node reachable over HTTP GraphQL works. Apply the SDL in
[`schema/coding_schemas.graphql`](schema/coding_schemas.graphql) first:

```sh
defradb client schema add --file schema/coding_schemas.graphql
```

The client writes to five collections — `CodingProject`, `CodingSession`,
`CodingMessage`, `CodingAction` and `CodingTokenUsage` — and reads back
`CodingSession` / `CodingMessage` / `CodingProject` documents to resume from
checkpoints. `message_id`, `session_id`, `usage_id` and `path` carry unique
indexes; re-ingesting the same source data collides with the existing rows
instead of duplicating them.

Point the client at the endpoint with `--graphql-endpoint`,
`CODING_DATA_GRAPHQL_ENDPOINT`, or `graphql_endpoint` in the config file. There
is no built-in default: the endpoint is required and the client exits with
`graphql_endpoint is required` if it is missing.

## Configuration

Precedence, lowest to highest: built-in defaults → config file → environment
variables → CLI flags.

The config file is TOML. It is read from `--config <path>` when given, else
from `~/.coding-data/config.toml` when that file exists:

```toml
source_dir = "~/.claude"
data_dir = "~/.coding-data/data"
sqlite_path = "~/.codex/state_5.sqlite"
source_host = "my-laptop"
graphql_endpoint = "http://127.0.0.1:9181/api/v0/graphql"
batch_size = 50
backfill_workers = 4
scan_interval_secs = 5
reconcile_interval_secs = 60
```

| Config key | Env var | CLI flag | Default |
| --- | --- | --- | --- |
| `source_dir` | `CODING_DATA_SOURCE_DIR` | `--source-dir` | per-source, see above |
| `data_dir` | `CODING_DATA_DATA_DIR` | `--data-dir` | `~/.coding-data/data` |
| `sqlite_path` | `CODING_DATA_SQLITE_PATH` | `--sqlite-path` | `~/.codex/state_5.sqlite` for Codex, unset otherwise |
| `source_host` | `CODING_DATA_SOURCE_HOST` | `--source-host` | the machine's hostname (`HOSTNAME`, `COMPUTERNAME`, then `hostname`), else `unknown` |
| `graphql_endpoint` | `CODING_DATA_GRAPHQL_ENDPOINT` | `--graphql-endpoint` | empty — required |
| `batch_size` | `CODING_DATA_BATCH_SIZE` | `--batch-size` | `50` |
| `backfill_workers` | `CODING_DATA_BACKFILL_WORKERS` | `--backfill-workers` | `4` |
| `scan_interval_secs` | `CODING_DATA_SCAN_INTERVAL_SECS` | `--scan-interval-secs` | `5` |
| `reconcile_interval_secs` | `CODING_DATA_RECONCILE_INTERVAL_SECS` | `--reconcile-interval-secs` | `60` for Codex, unset otherwise |

`--source <claude|codex|gemini|grok>` selects the source (default `claude`).
`source_host` is stamped onto every session, message and usage row so a shared
store can tell which machine produced a record; set it explicitly when the
hostname is not meaningful.

Ingest checkpoints live under `data_dir/<source>/ingest-state.json`. Deleting
that file forces a full re-read of the source; the store's unique indexes keep
the re-ingest idempotent.

## Subcommands

### `backfill`

Read every discovered file once, in parallel across `backfill_workers` tasks,
and write the results in batches of `batch_size`.

```sh
coding-data --source claude --graphql-endpoint http://127.0.0.1:9181/api/v0/graphql backfill
coding-data --source codex  --graphql-endpoint ... backfill --project example-repo
coding-data --source gemini --graphql-endpoint ... backfill --dry-run
```

- `--project <substring>` limits the run to files whose project path matches.
- `--dry-run` parses and batches without sending mutations.

For Codex, `backfill` also runs one reconciliation pass against the SQLite
catalog to pick up titles, archived flags and subagent roles.

### `watch`

Poll the source directory every `scan_interval_secs`, read only the bytes
appended since the last checkpoint, and write them out. Files that fail to
parse are deferred with exponential backoff (15s doubling to a 5-minute cap)
and retried when they change; a file that keeps failing is marked flapping and
backed off further. Checkpoints are persisted after each successful scan, so a
restart resumes where it stopped.

```sh
coding-data --source claude --graphql-endpoint ... watch
coding-data --source claude --graphql-endpoint ... watch --sources claude,codex
```

`--sources` takes a comma-separated list and runs one watcher per source
against the same store; with no value it watches the `--source` source only.
For Codex, a reconciler also polls the SQLite catalog every
`reconcile_interval_secs`.

`watch` serves Prometheus metrics on `0.0.0.0:$CODING_DATA_METRICS_PORT`
(default `9196`) at `/metrics`:

- `coding_data_watch_scans_total{status="success"|"failed"}`
- `coding_data_deferred_watch_files` (current backlog)
- `coding_data_deferred_watch_events_total`
- `coding_data_graphql_retries_total`
- `coding_data_last_success_unixtime`

`backfill` and `repair` do not start the metrics server.

### `repair`

Query the store for sessions whose `message_count` exceeds the number of
`CodingMessage` documents actually linked to them, reset the local checkpoint
for the matching source files, and re-ingest them.

```sh
coding-data --source claude --graphql-endpoint ... repair
coding-data --source claude --graphql-endpoint ... repair --session <session-id>
coding-data --source claude --graphql-endpoint ... repair --dry-run
```

- `--session <id>` restricts the repair to one session.
- `--dry-run` reports the gaps without touching state or re-ingesting.

## Building

```sh
cargo build --release
cargo test
```

The binary is `target/release/coding-data`.

## License

MIT — see [LICENSE](LICENSE).
