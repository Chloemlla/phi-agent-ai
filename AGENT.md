# Working on phi-agent

This file is for coding agents and contributors. It records the rules that are
easy to violate by accident.

## Non-negotiable rules

1. **Source code is English.** Identifiers, comments, log keys and commit
   messages are English. Do not introduce Chinese string literals in Rust,
   TypeScript or tests.
2. **Every human-visible string goes through i18n.** In Rust use
   `phi_i18n::t("English message")` or
   `phi_i18n::t_args("Hello {name}", &[("name", name)])` (a crate may
   `use phi_i18n as i18n;`). The English text is the message id;
   `src-tauri/crates/phi-i18n/src/lib.rs` is the only file that contains
   translations. Add the Chinese translation there when you add a message.
   Later UI work should follow the same rule (English source, translated
   catalog).
3. **Headless mode is mandatory.** Every shared feature must be usable and
   tested without the desktop UI. `phi-headless` is the reference shell:
   `cargo run -p phi-headless -- <command>`.
   A feature is not done until it has been exercised through headless mode or
   through an integration test that composes the same `Node`.
4. **Do not force a local build.** Some development machines only have a
   partial Rust toolchain (for example, no MSVC linker). Do not spend time
   fighting the local linker. Push a branch and read GitHub Actions with `gh`:

   ```bash
   gh run list --branch <branch> --limit 5
   gh run watch <run-id>
   gh run view <run-id> --log-failed
   ```

   `cargo fmt` is safe locally. `cargo check`/`cargo test` may not be.
5. **README is user-facing only.** `README.md` and `README.zh-CN.md` describe
   what a person can do and experience: capabilities, safety promises,
   requirements, quick start, FAQ and roadmap. Keep the two language versions
   in sync, and make every claim true for the shipped desktop UI. Do not put
   architecture, plugin/slot/kernel internals, headless CLI usage, wire
   protocols, environment variables, data paths, exit codes or CI details in
   the READMEs; those belong here or in `docs/`.
6. **Scope additions are recorded in `docs/`.** Anything we add that the
   original design did not have — a UI surface, protocol, command or product
   capability — goes into `docs/` as a written record: append a section to
   `docs/80-roadmap/scope-additions.md`, or start a new numbered file when the
   area is large enough to stand alone, and register new files in
   `docs/README.md`. Removal or replacement is recorded too, with the reason
   and the successor. Do not grow README or this file with those lists; keep
   them pointing at the docs. The Intelligent UI experiment and its removal
   live in `docs/80-roadmap/scope-additions.md`.

## Layout

The Rust code is a workspace under `src-tauri/`. Crates exist to enforce the
dependency direction at compile time:

- `crates/phi-kernel/` — mechanisms only (event bus, lifecycle, registry,
  mechanism ports, atomic file replacement). No domain vocabulary.
- `crates/phi-domain/` — messages, domain events and session read models.
- `crates/phi-contracts/` — ports, typed service handles, provider profiles,
  stream events, extension-slot names.
- `crates/phi-plugins/` — built-in implementations: model gateway, protocol
  adapters, tools, policy, approvals, session projection, sandbox.
- `crates/phi-adapters/` — filesystem journal/blob/config, system clock/ids,
  JSONL sinks, data-directory lock and cross-process control files.
- `crates/phi-i18n/` — language selection and the translation catalog.
- `crates/phi-node/` — shell-independent composition. Desktop and headless
  both call `compose` and use the same `Node`.
- `crates/phi-headless/` — headless shell. Durable events and live model deltas
  are JSONL on stdout; logs and summaries are on stderr.
- `src-tauri/src/` — desktop shell only (Tauri commands, sinks, entry point).
  No Tauri type may appear outside this crate.
- `src/` — SolidJS desktop UI.

The intended dependency graph is `kernel → domain → contracts → plugins →
adapters → node → shells`. Never add a dependency from a lower crate to a
higher one.

## Headless workflow

```bash
# One-shot run (a provider must be configured first; without one the run fails honestly)
phi-headless --data-dir .phi --workspace . --message "list the workspace"

# Pick a provider and a language
phi-headless --lang zh-CN run --provider prov_xxx --message "hello"
```

Exit codes: `0` success, `3` aborted, `4` failed, `5` awaiting
reconciliation, `2` usage/configuration error.

Only one writable node may use a data directory at a time; `compose` takes an
exclusive lock (`<data-dir>/phi.lock`). Read-only commands (`sessions show`,
`status`, provider reads) project the journal without locking. `approve` and
`cancel` hand a command file to the running process through
`<data-dir>/control/`, because a separate CLI invocation cannot reach another
process's memory.

Providers are durable profiles in `<data-dir>/providers.json`. Credentials are
references (`env:NAME`, `literal:VALUE`, `keychain:NAME`). Read APIs only ever
return a redacted hint. The keychain adapter is not implemented yet and fails
honestly.

## Model gateway

Three wire protocols are supported: `anthropic_messages`, `openai_chat` and
`openai_responses`, each with SSE streaming. Streaming deltas are transient and
go to the shell sink; only the final assistant message is journaled.

## Memory

Long-term memory is an optional built-in plugin (`memory.store` slot). A node
starts without it; retrieval degrades to no memory and writes fail honestly.

- Facts, source episodes and the inverted index live in
  `<data-dir>/memory/memory.db` (SQLite, WAL). The session journal only receives
  metadata-only `Memory*` events (id, scope, trust, counts, reason) — fact text
  never enters the journal.
- Every write passes the admission barrier: scope context, length bounds, valid
  interval, secret redaction, then a duplicate check. A bare secret is rejected;
  a secret embedded in a larger fact is redacted and the record is marked
  `redacted`.
- Retrieval is BM25 over a deterministic tokenizer (Latin words plus CJK
  bigrams). Visibility comes from the host-built `Principal`; scope filters can
  only narrow it. Agent memories are project-bound, run memories are run-bound,
  and user memories cross projects for the same user. The local user is the
  node id; project identity is the resolved workspace path (an explicit
  `--workspace` wins for that process).
- The agent receives `memory_search` and `memory_write` tools. Recalled facts are
  injected into the system prompt as labelled, untrusted data with a 500 ms
  deadline; retrieval failure degrades to no memory.
- Agent-authored facts are stored with trust `model`; admission checks form and
  secrets, not truth. The prompt labels all recalled facts as data, never as
  instructions. Tool audit previews for memory tools omit or redact the fact;
  the assistant tool-use message itself remains conversation data.
- `forget` is a full-chain delete: facts, index rows and episodes. The audit
  event keeps scope, counts and reason, never content. Conversation history is
  separate session data and is not rewritten.
- `search` applies execution-scope visibility. `list`, `get`, `invalidate`,
  `supersede`, `correct` and `forget` are owner-level management operations
  authorized by the principal's user id; the settings page and CLI may manage
  the owner's memories across projects and runs.
- Memory writes take the same single-writer data-directory lock as every other
  write; read-only memory commands do not lock.
- Retrieval quality is tracked by a golden set:
  `cargo test -p phi-plugins --test memory_golden -- --ignored --nocapture`.
  CI runs it non-blocking at MVP stage; scope leakage is a hard zero in the
  regular suite.

```bash
phi-headless memory add --fact "The user prefers pnpm" --scope project
phi-headless memory search "package manager"
phi-headless memory list --state active
phi-headless memory forget <id> --reason "outdated"
phi-headless memory stats
```

## Workbench view

The main surface is a task workbench, not a chat log. The agent can call the
`ui_update` tool to replace the view tree rendered beside the conversation.
Views are declarative JSON data (`src-tauri/crates/phi-contracts/src/ui.rs`,
version 1): layout, content and action kinds are allowlisted and the node
validates every tree before it becomes a `UiViewUpdated` event. The shell
renders the tree with a Solid component registry (`src/workbench/`); it never
evaluates model-authored code. Buttons and forms can only submit a message or
open an http(s) URL.

The desktop shell offers the tool on every run; the agent decides whether a
visual layout helps, so there is no user-facing workbench toggle. The protocol
prompt lives in `src-tauri/assets/workbench-system-prompt.txt`. Headless runs
can still request a plain chat run without the tool to exercise the non-workbench
path.

## Tests and CI

- Rust unit tests live next to the code; integration tests live in
  `src-tauri/crates/phi-headless/tests/`.
- Headless smoke tests use `env!("CARGO_BIN_EXE_phi-headless")`.
- CI runs `cargo fmt --all -- --check`, frontend typecheck/build, the full
  workspace test suite on Windows, and a Linux job for every crate except the
  desktop shell (`cargo test --workspace --exclude phi-agent`).
