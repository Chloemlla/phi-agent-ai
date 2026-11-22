<div align="center">
  <img src="assets/banner.svg" alt="phi-agent — Your agent. Your machine. Your rules." width="100%">
</div>

<div align="center">

**Your agent. Your machine. Your rules.**

[![License](https://img.shields.io/badge/license-GPL--3.0-6ea8fe?style=flat-square)](LICENSE)
[![Status](https://img.shields.io/badge/status-early%20access-e3b341?style=flat-square)](#roadmap)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-2c313a?style=flat-square)](#requirements)

**English** · [简体中文](README.zh-CN.md)

</div>

---

phi-agent is a desktop AI agent that does real work on your computer — reading repositories, editing files, running commands, and showing you exactly what happened. It runs on your machine, keeps your sessions there, and asks before it changes anything.

No account. No upload. No surprises.

> **Ask. Approve. Watch it work. Take it back.**

## Why phi-agent?

Most agents make you choose between convenience and control. phi-agent refuses to trade one for the other.

|  | Cloud agents | IDE copilots | **phi-agent** |
|---|:---:|:---:|:---:|
| Your data stays on your device | — | Partial | **Yes, by default** |
| Works offline | — | Partial | **Yes** (except remote model calls) |
| Can read, edit, and run commands | Sometimes | Mostly edits | **Yes, with approval** |
| Every run is saved and reviewable | Rarely | — | **Yes, by default** |
| Bring your own model | — | — | **Yes** |
| Continue a task on another device | — | — | **Planned** |

## What it feels like

**1. Tell it what you want.**
*“Find out why the build is flaky and fix it.”*

**2. Approve what matters.**
Reading your workspace is automatic. Writing files and running shell commands ask first — and the approval is tied to the exact command and arguments.

**3. Watch it work.**
Messages, tool calls, approval requests, and results appear in the conversation as each step completes. The whole run stays in your session, so you can look back at what happened.

**4. Take it back.**
Cancel at any time. The agent keeps working until the task is done or you stop it; steps and token usage are shown live. If the app closes mid-task, the run is marked for review instead of being silently retried.

## Built for people who own their tools

| | |
|---|---|
| 🔒 **Local-first**<br>Sessions and API keys stay on your machine. | 🧾 **Recorded**<br>Every run is saved and can be reviewed later. |
| 🛑 **Human approval**<br>Writes and commands wait for your explicit OK. | 📊 **Visible usage**<br>Steps and token usage are shown live for every run. |
| 🧠 **Bring your own model**<br>Use your own Anthropic- or OpenAI-compatible model endpoints and keys. | 🔁 **Crash-safe**<br>Interrupted work is marked for review, never auto-retried. |
| 🖥️ **One workspace, one boundary**<br>File tools stay inside the folder you choose. | 🛰️ **Multi-device**<br>Continue a task from another device — on the roadmap. |

## Bring your own model

phi-agent works with Anthropic Messages and OpenAI-compatible endpoints. Add one or more providers in **Settings → Model providers**: enter the endpoint and key, then pick the models you want from the list fetched from the provider. Keys stay on your machine and are sent only to the endpoints you configure. One provider is active at a time.

The composer shows the current model. Switch models without opening Settings, and choose how much thinking effort to spend on the next message. Replies render Markdown, including code blocks and tables.

Without a provider, a run fails with a clear message instead of inventing output. Point a provider at a local OpenAI-compatible runtime (Ollama, LM Studio, vLLM) to stay fully offline.

## Answers that can lay themselves out

When prose is not the best shape for an answer, the agent can put a small, working interface directly in its reply — a checklist you can tick, a plan with progress, a filterable set of metrics, a chart, or a short form that sends your choices back. It renders inside the message flow and styled to match the rest of the app.

The controls are real HTML: sliders, choices, checklists, steppers, collapsible details and tables. Their state stays in the page while you explore; nothing is sent to you or the model until you press a send button or submit the form. Charts are drawn from ordinary tables in the reply.

Nothing model-written runs: executable scripts, styles, iframes and event handlers are removed, and every visible element is rebuilt from a fixed allowlist. Links open in your browser; images must use https URLs from the conversation.

## Memory you can inspect

The agent can save durable facts — preferences, project conventions, decisions — and recall them in later runs. Every fact records where it came from and how much that source is trusted; unverified sources carry lower confidence and are labelled.

Open **Settings → Memory** to search what the agent remembers, add a fact yourself, or delete one permanently. Deleting a fact also erases its stored source excerpt and search index from memory. Past conversation transcripts are a separate record and are not rewritten. Values that look like credentials are redacted before a fact is stored in memory, and a bare secret is refused.

Memory is stored on your machine — there is no separate memory service. Recalled facts are shared with the model provider you configured, just like the rest of the conversation. If recall fails, the agent continues without it rather than blocking the task.

## Quick start

phi-agent is in early access. There are no packaged installers yet — build it from source.

### Requirements

- Rust 1.89+ (MSVC toolchain on Windows)
- [Bun](https://bun.sh) and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- Windows: Visual Studio Build Tools with the “Desktop development with C++” workload

### Run

```bash
bun install
bun run tauri dev
```

On Android, install the Android SDK/NDK from the Tauri 2 prerequisites and run `bun run tauri android dev`. The UI adapts to phone widths with a drawer sidebar, a touch-friendly composer and safe-area insets.

On first launch, phi-agent creates a `phi-workspace` folder in your Documents directory. You can change it in the app.

## Safety you can see

Giving an agent the ability to edit files and run commands is serious. phi-agent treats it that way:

- **Deny by default.** Unknown tools and paths outside the workspace are refused.
- **Approval is bound to the action.** Your OK applies to the exact tool, workspace, and normalized arguments. Change an argument — or the file you approved — and the approval no longer matches.
- **File tools stay in the workspace.** The folder you choose is the boundary for reading and writing files.
- **Shell commands use your permissions.** Command execution is not sandboxed like a virtual machine, so the approval prompt is the real safety boundary. Read it before you approve.
- **Cancellation is real.** Commands that may have already had an effect are marked “outcome unknown” instead of pretending otherwise.
- **Usage is visible.** Every model call records its input and output tokens in the session; there is no hidden pricing or meter. Stop any run with Cancel.

## Roadmap

- [x] **v0.1 — Desktop** · real coding tasks, approvals, cancellation, crash recovery
- [x] **Memory MVP** · durable facts the agent can recall; inspect, search and delete them in Settings
- [ ] **v1 — Beyond one window** · work from more than one device, installable extensions, richer memory
- [ ] **Later** · task handoff across devices, extensions marketplace

## FAQ

<details>
<summary><b>Does phi-agent upload my data?</b></summary>

No. Sessions and API keys are local by default. Network traffic goes only to the model endpoint you configure.
</details>

<details>
<summary><b>Can I use it offline?</b></summary>

The app itself runs offline. Model calls need a provider; point one at a local OpenAI-compatible runtime (for example Ollama or LM Studio) and nothing leaves your machine.
</details>

<details>
<summary><b>Is it sandboxed like a virtual machine?</b></summary>

Not yet. File tools are confined to the workspace folder, but shell commands run with your own user permissions. Treat the approval prompt and your workspace choice as the real boundary.
</details>

<details>
<summary><b>Which models are supported?</b></summary>

Anthropic Messages and OpenAI-compatible endpoints (Chat Completions or Responses), including local runtimes that expose an OpenAI-compatible API.
</details>

<details>
<summary><b>Where are my API keys stored?</b></summary>

On your machine. The app stores a reference — an environment variable name (`env:NAME`) or, for development, a literal value in its local data folder. Values are only sent to the endpoint you configure.
</details>

<details>
<summary><b>What happens if I close the app mid-task?</b></summary>

The run stops and is marked for review the next time you open the app. It is never retried automatically, because a command may already have changed something.
</details>

## Contributing

phi-agent is built in the open. Issues, design discussions, and pull requests are welcome — start with [CONTRIBUTING.md](CONTRIBUTING.md). For security reports, please follow [SECURITY.md](SECURITY.md).

## License

[GNU General Public License v3.0](LICENSE) · `GPL-3.0-only`

<div align="center"><sub>Your agent. Your machine. Your rules.</sub></div>
