# AGENTS.md — coding-data

Ingest of Claude Code, Codex, Gemini and Grok session history into a DefraDB
GraphQL store.

- Codex `CodingMessage.message_id` uses the durable `codex-raw-line/v1`
  identity. Never change it: checkpoint resets and full re-ingest must collide
  with already-stored rows. Replay normalization uses a separate transient
  `codex-replay/v1` identity within one ingest run.
- Preserve checkpoint semantics when touching `src/watch/` or `src/state.rs`.
- `schema/coding_schemas.graphql` must stay in sync with the fields the
  mutation builders in `src/mutations/builders/` write.
