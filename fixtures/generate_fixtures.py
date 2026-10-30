#!/usr/bin/env python3
"""Generate every fixture under fixtures/ from scratch.

Nothing is copied from a real session: project, repo, prompts, commands, token
counts and thread ids are all invented. Re-run to regenerate byte-for-byte:

    python3 fixtures/generate_fixtures.py

The Codex rollouts cover the parser paths the tests assert on: session_meta,
turn_context, user/assistant messages, both reasoning encodings, function
calls, function call output, custom tool calls, web search, token_count and
a `compacted` marker.
"""

import json
import os
import sqlite3

ROOT = os.path.dirname(os.path.abspath(__file__))
CODEX = os.path.join(ROOT, "codex-home")
REPO = "/home/user/src/example/example-repo"
GIT_URL = "git@example.com:octo-org/example-repo.git"
CLI_VERSION = "0.116.0"
MODEL = "gpt-5.4"


def j(obj):
    """One compact JSONL line, matching Codex's own spacing."""
    return json.dumps(obj, separators=(",", ":"))


def write_lines(path, lines):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")


def write_json(path, obj, indent=2):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(obj, fh, indent=indent)
        fh.write("\n")


# --- Codex line builders --------------------------------------------------

def session_meta(sid, ts, agent_role=None):
    payload = {
        "id": sid,
        "timestamp": ts,
        "cwd": REPO,
        "cli_version": CLI_VERSION,
        "model": MODEL,
        "git": {"commit_hash": "abc1234", "branch": "main", "repository_url": GIT_URL},
    }
    if agent_role:
        payload["source"] = {
            "subagent": {
                "thread_spawn": {"parent_thread_id": "session-alpha", "agent_role": agent_role}
            }
        }
    else:
        payload["source"] = "cli"
    return j({"timestamp": ts, "type": "session_meta", "payload": payload})


def turn_context(ts, turn_id, effort="high"):
    return j({
        "timestamp": ts,
        "type": "turn_context",
        "payload": {"turn_id": turn_id, "cwd": REPO, "model": MODEL, "effort": effort},
    })


def user_message(ts, text):
    return j({
        "timestamp": ts,
        "type": "response_item",
        "payload": {
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": text}],
        },
    })


def assistant_message(ts, text, turn_id=None):
    payload = {
        "type": "message",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text}],
    }
    if turn_id:
        payload["internal_chat_message_metadata_passthrough"] = {"turn_id": turn_id}
    return j({"timestamp": ts, "type": "response_item", "payload": payload})


def reasoning_item(ts, text):
    """New-style reasoning: a `reasoning` response item with a summary block."""
    return j({
        "timestamp": ts,
        "type": "response_item",
        "payload": {"type": "reasoning", "summary": [{"type": "summary_text", "text": text}]},
    })


def agent_reasoning_event(ts, text):
    """Old-style reasoning: an `agent_reasoning` event message."""
    return j({
        "timestamp": ts,
        "type": "event_msg",
        "payload": {"type": "agent_reasoning", "text": text},
    })


def function_call(ts, call_id, name, arguments):
    return j({
        "timestamp": ts,
        "type": "response_item",
        "payload": {
            "type": "function_call",
            "name": name,
            "call_id": call_id,
            "arguments": j(arguments),
        },
    })


def function_call_output(ts, call_id, output):
    return j({
        "timestamp": ts,
        "type": "response_item",
        "payload": {"type": "function_call_output", "call_id": call_id, "output": output},
    })


def custom_tool_call(ts, call_id, name, patch):
    return j({
        "timestamp": ts,
        "type": "response_item",
        "payload": {"type": "custom_tool_call", "name": name, "call_id": call_id, "input": patch},
    })


def web_search_call(ts, call_id, query):
    return j({
        "timestamp": ts,
        "type": "response_item",
        "payload": {"type": "web_search_call", "call_id": call_id, "query": query},
    })


def token_count(ts, input_tokens, output_tokens, cached, reasoning, total):
    usage = {
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "cached_input_tokens": cached,
        "reasoning_output_tokens": reasoning,
        "total_tokens": total,
    }
    return j({
        "timestamp": ts,
        "type": "event_msg",
        "payload": {"type": "token_count", "info": {"last_token_usage": usage, "total_token_usage": usage}},
    })


def user_message_event(ts, text):
    return j({
        "timestamp": ts,
        "type": "event_msg",
        "payload": {"type": "user_message", "message": text, "images": [], "local_images": []},
    })


def compacted(ts):
    return j({"timestamp": ts, "type": "compacted", "payload": {"message": "history compacted"}})


# --- Codex rollouts -------------------------------------------------------

def rollout(day, hhmm, n):
    """A Codex rollout filename: rollout-<date>T<time>-<uuid>.jsonl."""
    return f"rollout-2026-03-{day}T{hhmm}-0a1b2c3d-1111-4aaa-8bbb-{n:012d}.jsonl"


ALPHA_1 = rollout("13", "09-00-00", 1)
ALPHA_2 = rollout("13", "09-05-00", 2)
BETA = rollout("14", "10-00-00", 3)
GAMMA = rollout("15", "11-00-00", 4)
DELTA = rollout("16", "12-00-00", 5)
EPSILON = rollout("17", "08-00-00", 6)

PROMPT_ONE = "Add a health check endpoint to the widget service."
PATCH = (
    "*** Begin Patch\n"
    "*** Update File: src/health.rs\n"
    "@@\n"
    '-pub fn healthz() -> &\'static str { "ok" }\n'
    '+pub fn healthz() -> &\'static str { "healthy" }\n'
    "*** End Patch\n"
)


def build_codex_rollouts():
    write_lines(os.path.join(CODEX, "sessions/2026/03/13", ALPHA_1), [
        session_meta("session-alpha", "2026-03-13T09:00:00Z"),
        turn_context("2026-03-13T09:00:01Z", "turn-1"),
        user_message("2026-03-13T09:00:02Z", PROMPT_ONE),
        user_message_event("2026-03-13T09:00:02Z", PROMPT_ONE),
        reasoning_item("2026-03-13T09:00:03Z", "The router already has a health module; reuse it."),
        function_call("2026-03-13T09:00:04Z", "call-exec-1", "exec_command",
                      {"cmd": "cargo test --quiet", "workdir": REPO}),
        function_call_output("2026-03-13T09:00:09Z", "call-exec-1",
                             "test result: ok. 12 passed; 0 failed"),
        token_count("2026-03-13T09:00:10Z", 1200, 180, 900, 64, 1380),
        assistant_message("2026-03-13T09:00:11Z",
                          "Added a /healthz handler and a test for it.", turn_id="turn-1"),
    ])

    write_lines(os.path.join(CODEX, "sessions/2026/03/13", ALPHA_2), [
        session_meta("session-alpha", "2026-03-13T09:05:00Z"),
        agent_reasoning_event("2026-03-13T09:05:01Z",
                              "Wire the handler into the router and re-run the suite."),
        custom_tool_call("2026-03-13T09:05:02Z", "call-patch-1", "apply_patch", PATCH),
        assistant_message("2026-03-13T09:05:06Z",
                          "Wired the handler into the router.", turn_id="turn-2"),
    ])

    write_lines(os.path.join(CODEX, "sessions/2026/03/14", BETA), [
        session_meta("session-beta", "2026-03-14T10:00:00Z", agent_role="worker"),
        user_message("2026-03-14T10:00:02Z", "Investigate the flaky widget test."),
        function_call("2026-03-14T10:00:03Z", "call-shell-1", "shell",
                      {"command": "cargo test widget -- --nocapture"}),
        function_call_output("2026-03-14T10:00:12Z", "call-shell-1",
                             "test result: ok. 3 passed; 0 failed"),
        assistant_message("2026-03-14T10:00:14Z",
                          "The flake is a timing assumption in the retry helper."),
    ])

    write_lines(os.path.join(CODEX, "sessions/2026/03/15", GAMMA), [
        session_meta("session-gamma", "2026-03-15T11:00:00Z"),
        user_message("2026-03-15T11:00:02Z", "Summarize this week's changelog."),
        web_search_call("2026-03-15T11:00:03Z", "call-search-1",
                        "widget service changelog conventions"),
        token_count("2026-03-15T11:00:05Z", 640, 95, 0, 0, 735),
        assistant_message("2026-03-15T11:00:06Z",
                          "Three fixes landed: health check, retry helper, changelog."),
    ])

    write_lines(os.path.join(CODEX, "archived_sessions", DELTA), [
        session_meta("session-delta", "2026-03-16T12:00:00Z"),
        compacted("2026-03-16T12:00:01Z"),
        user_message("2026-03-16T12:00:02Z", "Close out the archived session."),
        assistant_message("2026-03-16T12:00:03Z", "Archived."),
    ])


# --- Codex thread catalog -------------------------------------------------

THREADS_SCHEMA = """
CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    rollout_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    source TEXT NOT NULL,
    model_provider TEXT NOT NULL,
    cwd TEXT NOT NULL,
    title TEXT NOT NULL,
    sandbox_policy TEXT NOT NULL,
    approval_mode TEXT NOT NULL,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    has_user_event INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    archived_at INTEGER,
    git_sha TEXT,
    git_branch TEXT,
    git_origin_url TEXT,
    cli_version TEXT NOT NULL DEFAULT '',
    first_user_message TEXT NOT NULL DEFAULT '',
    agent_nickname TEXT,
    agent_role TEXT,
    memory_mode TEXT NOT NULL DEFAULT 'enabled',
    model TEXT,
    reasoning_effort TEXT
);
"""

SUBAGENT_SOURCE = j({"subagent": {"thread_spawn": {"parent_thread_id": "session-alpha"}}})


def thread(sid, rollout, created, updated, title, first, *, archived=0,
           source="cli", nickname=None, role=None, effort=None):
    return {
        "id": sid,
        "rollout_path": rollout,
        "created_at": created,
        "updated_at": updated,
        "source": source,
        "cwd": REPO,
        "title": title,
        "archived": archived,
        "git_sha": "abc1234",
        "git_branch": "main",
        "git_origin_url": GIT_URL,
        "first_user_message": first,
        "agent_nickname": nickname,
        "agent_role": role,
        "model": MODEL,
        "reasoning_effort": effort,
    }


THREADS = [
    thread("session-alpha", f"/home/user/.codex/sessions/2026/03/13/{ALPHA_1}",
           1773392400, 1773392700, "Add a health check endpoint", PROMPT_ONE, effort="high"),
    thread("session-beta", f"/home/user/.codex/sessions/2026/03/14/{BETA}",
           1773482400, 1773483300, "Investigate the flaky widget test",
           "Investigate the flaky widget test.", archived=1, source=SUBAGENT_SOURCE,
           nickname="Scout", role="worker", effort="medium"),
    thread("session-gamma", f"/home/user/.codex/sessions/2026/03/15/{GAMMA}",
           1773568800, 1773569100, "Summarize the changelog",
           "Summarize this week's changelog.", effort="low"),
    thread("session-delta", f"/home/user/.codex/archived_sessions/{DELTA}",
           1773655200, 1773655500, "Archived session",
           "Close out the archived session.", archived=1),
    thread("session-epsilon", f"/home/user/.codex/sessions/2026/03/17/{EPSILON}",
           1773738000, 1773738300, "Review the retry helper",
           "Review the retry helper.", source=SUBAGENT_SOURCE,
           nickname="Scout", role="worker", effort="medium"),
]


def build_codex_catalog():
    path = os.path.join(CODEX, "state_5.sqlite")
    if os.path.exists(path):
        os.remove(path)
    connection = sqlite3.connect(path)
    try:
        connection.executescript(THREADS_SCHEMA)
        connection.executemany(
            """
            INSERT INTO threads (
                id, rollout_path, created_at, updated_at, source, model_provider,
                cwd, title, sandbox_policy, approval_mode, archived, git_sha,
                git_branch, git_origin_url, cli_version, first_user_message,
                agent_nickname, agent_role, model, reasoning_effort
            ) VALUES (
                :id, :rollout_path, :created_at, :updated_at, :source, 'openai',
                :cwd, :title, 'danger-full-access', 'never', :archived, :git_sha,
                :git_branch, :git_origin_url, '0.116.0', :first_user_message,
                :agent_nickname, :agent_role, :model, :reasoning_effort
            )
            """,
            THREADS,
        )
        connection.commit()
    finally:
        connection.close()


# --- Claude Code ----------------------------------------------------------

def build_claude():
    def line(ts, role, content, input_tokens, output_tokens):
        return j({
            "sessionId": "claude-example-session",
            "cwd": "/home/user/src/example/claude-project",
            "gitBranch": "main",
            "version": "1.0.0-test",
            "timestamp": ts,
            "message": {"role": role, "content": content,
                        "model": "claude-sonnet-test",
                        "usage": {"input_tokens": input_tokens,
                                  "output_tokens": output_tokens}},
        })

    write_lines(
        os.path.join(ROOT, "claude-home/projects/example-project/session-2026-03-27.jsonl"),
        [
            line("2026-03-27T10:00:00Z", "user",
                 "Draft a synthetic project summary in two short sentences.", 16, 0),
            line("2026-03-27T10:00:03Z", "assistant",
                 "Synthetic project summary: the service indexes sample notes and "
                 "returns deterministic test data.", 16, 18),
            line("2026-03-27T10:00:08Z", "user",
                 "List one safe follow-up task for the synthetic project.", 10, 0),
            line("2026-03-27T10:00:11Z", "assistant",
                 "Safe follow-up task: add a synthetic health-check fixture for "
                 "regression coverage.", 10, 12),
        ],
    )


# --- Gemini CLI -----------------------------------------------------------

GEMINI_PROJECT = "/home/user/src/example/gemini-project"


def build_gemini():
    base = os.path.join(ROOT, "gemini-home")
    write_json(os.path.join(base, "projects.json"), {"projects": {GEMINI_PROJECT: "example-project"}})
    with open(os.path.join(base, "tmp/example-project/.project_root"), "w", encoding="utf-8") as fh:
        fh.write(GEMINI_PROJECT + "\n")

    def msg(mid, ts, kind, text, tokens):
        return {"id": mid, "timestamp": ts, "type": kind,
                "content": [{"text": text}], "model": "gemini-test",
                "tokens": tokens}

    write_json(
        os.path.join(base, "tmp/example-project/chats/session-2026-03-27.json"),
        {
            "sessionId": "gemini-example-session",
            "projectHash": "example-project",
            "startTime": "2026-03-27T11:00:00Z",
            "lastUpdated": "2026-03-27T11:00:14Z",
            "kind": "main",
            "summary": "Synthetic Gemini fixture conversation",
            "messages": [
                msg("msg-1", "2026-03-27T11:00:00Z", "user",
                    "Summarize the synthetic deployment notes.", {"input": 14}),
                msg("msg-2", "2026-03-27T11:00:05Z", "gemini",
                    "Synthetic deployment notes: the service exposes HTTP status and "
                    "stores deterministic sample rows.", {"output": 18}),
                msg("msg-3", "2026-03-27T11:00:10Z", "user", "Name one safe next step.",
                    {"input": 8}),
                msg("msg-4", "2026-03-27T11:00:14Z", "gemini",
                    "Safe next step: add a synthetic watcher fixture for regression "
                    "coverage.", {"output": 12}),
            ],
        },
    )

    write_json(
        os.path.join(base, "tmp/example-project/logs.json"),
        [
            {"sessionId": "gemini-log-session", "messageId": 1, "type": "user",
             "message": "Synthetic log prompt", "timestamp": "2026-03-27T11:10:00Z"},
            {"sessionId": "gemini-log-session", "messageId": 2, "type": "assistant",
             "message": "Synthetic log reply", "timestamp": "2026-03-27T11:10:02Z"},
        ],
    )


# --- Grok -----------------------------------------------------------------

GROK_PROJECT = "/home/user/src/example/grok-project"
GROK_DIR = "%2Fhome%2Fuser%2Fsrc%2Fexample%2Fgrok-project"


def build_grok():
    base = os.path.join(ROOT, "grok-home")
    session = os.path.join(base, "sessions", GROK_DIR, "session-1")
    child = os.path.join(base, "sessions", GROK_DIR, "session-child")

    write_json(os.path.join(base, "version.json"), {"version": "0.2.93"}, indent=None)

    write_lines(os.path.join(session, "chat_history.jsonl"), [
        j({"type": "system", "content": "You are Grok."}),
        j({"type": "user", "content": [{"type": "text", "text": "system reminder"}],
           "synthetic_reason": "system_reminder"}),
        j({"type": "user",
           "content": [{"type": "text", "text": "<user_query>\nbuild the adapter\n</user_query>"}]}),
        j({"type": "reasoning", "id": "rs_1",
           "summary": [{"type": "summary_text", "text": "Need to inspect sources"}],
           "status": "completed"}),
        j({"type": "assistant", "content": "I'll start by exploring.",
           "tool_calls": [{"id": "call-1", "name": "read_file",
                           "arguments": '{"target_file":"src/source.rs"}'}],
           "model_id": "grok-4.5", "reasoning_effort": "high"}),
        j({"type": "tool_result", "tool_call_id": "call-1", "content": "fn main() {}"}),
        j({"type": "assistant", "content": "Done.", "model_id": "grok-4.5",
           "reasoning_effort": "high"}),
    ])

    write_json(os.path.join(session, "signals.json"), {
        "contextTokensUsed": 12000,
        "toolsUsed": ["read_file", "run_terminal_command"],
        "primaryModelId": "grok-4.5",
    })

    write_json(os.path.join(session, "subagents/session-child/meta.json"), {
        "subagent_id": "session-child",
        "parent_session_id": "session-1",
        "child_session_id": "session-child",
        "subagent_type": "explore",
        "description": "scan sources",
    })

    write_json(os.path.join(session, "summary.json"), {
        "info": {"id": "session-1", "cwd": GROK_PROJECT},
        "session_summary": "Build Grok Adapter",
        "created_at": "2026-07-09T16:24:29.695338Z",
        "updated_at": "2026-07-09T16:30:00Z",
        "current_model_id": "grok-4.5",
        "git_remotes": ["https://github.com/octo-org/example-repo.git"],
        "head_commit": "abc1234",
        "head_branch": "main",
        "generated_title": "Build Grok Adapter",
        "agent_name": "grok-build-plan",
        "reasoning_effort": "high",
    })

    write_lines(os.path.join(session, "updates.jsonl"), [
        j({"timestamp": 1783614287, "method": "session/update", "params": {
            "sessionId": "session-1",
            "update": {"sessionUpdate": "agent_message_chunk",
                       "content": {"type": "text", "text": "hi"}},
            "_meta": {"totalTokens": 5000, "promptId": "prompt-1",
                      "agentTimestampMs": 1783614287000, "eventId": "e1"},
        }}),
    ])

    write_lines(os.path.join(child, "chat_history.jsonl"), [
        j({"type": "user", "content": "explore the repo"}),
        j({"type": "assistant", "content": "looking", "model_id": "grok-4.5"}),
    ])

    write_json(os.path.join(child, "summary.json"), {
        "info": {"id": "session-child", "cwd": GROK_PROJECT},
        "created_at": "2026-07-09T16:25:00Z",
        "current_model_id": "grok-4.5",
        "generated_title": "scan sources",
    })


def main():
    build_codex_rollouts()
    build_codex_catalog()
    build_claude()
    build_gemini()
    build_grok()
    print("fixtures regenerated under", ROOT)


if __name__ == "__main__":
    main()
