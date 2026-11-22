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
   `src-tauri/crates/phi-i18n/src/lib.rs` is the Rust translation catalog.
   Add the Chinese translation there when you add a message. The desktop UI has
   its own catalog in `src/i18n.ts`; a user-visible string added on the frontend
   must be added there too. Later UI work should follow the same rule (English
   source, translated catalog).
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

The dependency direction is enforced where it matters: `phi-kernel` and
`phi-domain` are leaves; `phi-contracts` depends on `phi-domain`; `phi-plugins`
and `phi-adapters` depend on the contracts/domain/kernel set; `phi-node`
composes everything; shells only talk to `phi-node`. Never add a dependency
from a lower crate to a higher one.

## Android shell

The Android build lives in `src-tauri/gen/android/` and reuses the same
`src-tauri/src/lib.rs` entry point.

- **TLS verification.** reqwest uses rustls on Android, which hands server
  certificates to `rustls-platform-verifier`. Its JVM component must be in the
  APK (`org.rustls:rustls-platform-verifier` in `app/build.gradle.kts`, with the
  version read from `Cargo.lock` by the root build script) and it must be
  initialized with the app's VM and context before the first HTTPS request
  (`src-tauri/src/android.rs`, called from `run()`). Without either half the
  verifier panics, and release builds abort on panic. The release build keeps
  the component through `proguard-rules.pro`.
- **Toolchain floor.** `compileSdk 37`, `minSdk 26` and Java/Kotlin target 17:
  the Lumen Crash SDK is compiled against 37 and uses `java.time`, which does
  not exist below API 26. Compose is enabled because the crash report screen is
  Compose; the compiler plugin version must match the Kotlin plugin version
  (both are pinned in the root `build.gradle.kts`).
- **Crash reporting.** `LumenCrash` is installed in `PhiApplication` and the
  SDK artifacts come from a GitHub release staged into
  `src-tauri/gen/android/lumen-crash-maven` by
  `node scripts/fetch-lumen-crash.mjs`; the pin lives in
  `src-tauri/gen/android/gradle.properties` (`lumenCrashSdkVersion`), and the
  staged tree is gitignored. The launcher intent belongs to
  `LaunchGateActivity`, not `MainActivity`: `TauriActivity.onCreate` builds the
  Rust webview, which is the code path a crash may have come from, so the crash
  screen must be shown without entering it. Rust panics are recorded by the
  hook installed in `src-tauri/src/android.rs` and webview errors by
  `src/platform/crash.ts`; both write files into `<files>/crash-inbox`, which the
  SDK turns into a report on the next launch. Webview entries must keep
  `fatal: false` — a recoverable JS error must never take the pending-report
  slot. Uploads are disabled (`crashReportBackendEnabled = false`): reports stay
  on the device until the user copies or shares one. See
  `docs/70-quality/android-crash-reporting.md`.
- **Release signing.** `app/build.gradle.kts` reads `keystore.properties`
  (gitignored) with `keyAlias`, `keyPassword`, `storeFile`, `storePassword` and
  an optional `storeType` (AGP's default applies when it is omitted). Without
  the file, release builds fall back to the debug keystore and must not be
  distributed. The keystore and its properties file are local credentials: back
  them up, because losing the key blocks updates to installed apps.

## Headless workflow

```bash
# One-shot run (a provider must be configured first; without one the run fails honestly)
phi-headless --data-dir .phi --workspace . run --message "list the workspace"

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
go to the shell sink; only the final assistant message is journaled. OpenAI
reasoning models (the GPT-6 family) use `max_completion_tokens` and reject
sampling parameters such as `temperature`.

Model metadata suggestions live in `src-tauri/assets/model-catalog.json`,
compiled into the binary. The file may be empty: unknown models stay manual
specs with only their id. Provider APIs supply the live model list. Metadata is
advisory only — the node records token usage, never prices. Catalogue entries
match on family patterns plus required variant markers, so an unrecorded point
release such as `gpt-6.5-luna` keeps its real version and variant. `ModelSpec.scale`
ranks capability from 1 (small, fast) to 5 (frontier) and caps delegation: an
agent may never call a model larger than itself. A per-message `thinking` effort
(`off`/`low`/`medium`/`high`/`xhigh`/`max`) maps to Anthropic's adaptive thinking plus
`output_config.effort`, OpenAI Chat's `reasoning_effort`, or Responses'
`reasoning.effort`; `off`/`None` omits the thinking fields — every current model
accepts them. Legacy Anthropic `budget_tokens` shapes are not supported.
Anthropic requests mark the system block with an ephemeral cache breakpoint,
and cached usage is parsed into `ModelCallReturned`.

## Runtime loop

The agent loop has no run-level step, token or wall-clock cap. It ends when the
model stops calling tools or when the user cancels. Per-message model and
thinking selection travel from the shell through `StartRunRequest` into every
`ModelRequest`; `default` keeps the provider's primary model. Token usage is
recorded per model call; prices are not computed.

Two failure modes are deliberately recoverable rather than fatal:

- A tool call whose arguments are not valid JSON is never executed. The
  assistant text is journaled, a paired `tool_result` error names the tool and
  parser error, and the loop lets the model re-issue the call.
- A response cut off by the output limit is continued (tool calls from the cut
  response are not executed); text-only continuation prompts are ephemeral and
  never appear as user messages. After three truncation recoveries the run
  fails honestly and names the token limit.

If a model call fails after some text was already streamed, that partial text is
persisted instead of being dropped. `shell_exec` keeps its own per-command
timeout (default 120s, max 300s); the HTTP client keeps its request timeout.

Multi-agent and optional model surfaces are implemented in the backend; the
desktop surfaces are still being connected. See `docs/40-runtime/multi-agent.md`,
`docs/40-runtime/auxiliary-models.md`, `docs/40-runtime/decision-model.md` and
ADR-0012–0016. Key entry points:

- Delegation: `AgentCreated/AgentStatusChanged/AgentReported`, per-agent
  `threadId`, `RunScheduled.parentRunId`, `SessionView.agents`, and the
  `delegate` / `wait_for_agents` / `send_message` / `cancel_agent` tools. Depth
  is 1; a subagent cannot delegate. Lead chooses the model from the active
  provider roster and `ModelSpec.scale` caps it. `shared` subagents take the
  single workspace write lease; `read_only` subagents never see write tools.
- Image generation: `<data-dir>/image.json`, the optional `image.generate`
  port and the `image_generate` tool. The user selects the model; the tool has
  no model parameter. Assets are content-addressed under
  `<data-dir>/assets/`. Headless: `image configure|show|generate|list|delete`.
- Decisions: `<data-dir>/decision.json`, the optional `decision.evaluate` port
  (Jev-compatible `noul`/`choice`/`score`), approval triage and long tool-output
  compaction. Failures and low confidence always fall back to the human or to
  deterministic head/tail truncation. Headless: `decision configure|show|test`.
- Text auxiliary roles: `<data-dir>/auxiliary.json`, `session_title`,
  `memory_maintenance`, `summarizer`. They are service calls, not Runs.
  `Node::settle_auxiliary` runs title generation and threshold-triggered memory
  maintenance after a turn; headless `run` calls it. Headless: `roles list|set|clear`
  and `memory maintain [--dry-run|--apply]`.
- The legacy `Task` entity, `TaskCreated` event and `taskId` field were deleted
  outright (ADR-0012); runs no longer carry a task id. Do not add a
  `strategy.orchestration` slot until a replaceable orchestrator is actually
  needed.

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

## Inline interface blocks

The agent can put a small, working interface directly into its reply instead
of into a separate panel. It writes HTML inside a fenced `html` block; the
text around the fence is normal Markdown. A top-level raw HTML block is also
recognized (any complete tag, ending at the first blank line), so a model that
writes `<div>` or `<span>` directly does not leak markup into the prose. The
live stream and the durable message use the same rules, including non-HTML
fences, whose contents stay prose. Every run receives the guidance in
`src-tauri/assets/inline-ui-system-prompt.txt`; there is no per-message or
per-run switch, and the node stores the reply verbatim, so headless mode sees
the fence like any other text.

The block is HTML plus a deliberately small layer:

- layout primitives: `ui-col`, `ui-row`, `ui-box`, `ui-grid` with validated
  token attributes (`data-gap`, `data-pad`, `data-cols`, `data-align`,
  `data-justify`, `data-flex`, `data-bg`, `data-border`, `data-radius`,
  `data-tone`, `data-size`, `data-weight`, `data-text-align`); the model
  composes layout without arbitrary CSS;
- a JSON data island (`<script type="application/json" data-ui-state>` or a
  `data-ui-state` template) holds state; it is parsed with `JSON.parse` and
  never executed;
- `{{ expression }}` interpolates state into text and a few attributes, parsed
  by `src/components/uiExpression.ts` (a fixed grammar: arithmetic,
  comparisons, ternary, member/index access and a closed function set — never
  `eval` or `new Function`); SVG shape attributes accept the same
  expressions, and SVG colors may name host tokens (`info`, `muted`,
  `chart1`…);
- `data-ui-bind` wires inputs, `data-ui-set` changes state,
  `data-ui-send`/`data-ui-copy` act, `data-ui-show` hides, and
  `<template data-ui-each>` repeats markup;
- buttons and forms go through the same session turn as the composer.

The shell never injects model HTML. `src/components/RichHtml.tsx` parses the
block with an inert `DOMParser`, drops scripts, styles, iframes, objects and
event attributes (including inline `style`), and rebuilds allowlisted
elements, host-owned classes, layout attributes and attributes as Solid JSX.
Host CSS owns the look: layout, cards, callouts, badges, icons, metrics, steps,
key/value rows, chips, figures, captioned images, carousels and form controls.
`InlineChart.tsx` draws bar, line and area charts from a `table.ui-chart` with
static rows. Links must be `http(s)`/`mailto`; images must be `https` and are
expected to come from the conversation. Blocks render in the message flow,
including while the reply streams (`src/components/inlineUi.ts`); the durable
message goes through the same Markdown renderer. State is cached for the
session, keyed by the block source, so choices survive transcript refreshes.

There is no UI protocol, no live view event and no `ui_update` tool (the old
`UiViewUpdated` variant survives only so pre-2026-11-20 journals replay).
Model-authored JavaScript is never executed. A rich visual block is ordinary
HTML/CSS wrapped in `<template data-ui-rich>` inside the `html` fence; the
host sanitizes it and renders it in a script-disabled, same-origin iframe
(`src/components/RichSandbox.tsx`) with its own CSP, so the host document still
never receives model markup. Forms, external stylesheets, nested frames and
scripts are removed or blocked, and the frame has no network except https
images. There is still no map/photo search, and host components plus the
expression layer remain the default surface. The old workbench protocol and
its history are recorded in `docs/80-roadmap/scope-additions.md` §2.1–§2.2 and
§2.17.

## Tests and CI

- Rust unit tests live next to the code; integration tests live in the
  `tests/` directory of `phi-headless`, `phi-adapters` and `phi-plugins`.
- Headless smoke tests use `env!("CARGO_BIN_EXE_phi-headless")`.
- CI runs `cargo fmt --all -- --check`, frontend typecheck/build, the full
  workspace test suite on Windows, and a Linux job for every crate except the
  desktop shell (`cargo test --workspace --exclude phi-agent`).
