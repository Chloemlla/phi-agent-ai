//! Headless smoke tests.
//!
//! These run the real `phi-headless` binary, which is also how the desktop
//! shell is exercised in CI. The mock model makes them deterministic: no
//! network, no API key, no UI.

use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct TestEnv {
    data_dir: PathBuf,
    workspace: PathBuf,
}

impl TestEnv {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("phi-headless-{name}-{}", uuid::Uuid::new_v4()));
        let data_dir = root.join("data");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        Self {
            data_dir,
            workspace,
        }
    }

    fn base_command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_phi-headless"));
        command
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--workspace")
            .arg(&self.workspace)
            .args(["--quiet"])
            .env("ANTHROPIC_API_KEY", "")
            .env("PHI_API_KEY", "")
            .env("PHI_LANG", "en");
        command
    }

    fn command(&self, args: &[&str]) -> Output {
        self.base_command()
            .args(args)
            .output()
            .expect("phi-headless should start")
    }

    /// Start a run that keeps its stdin open, so an approval in `prompt` mode
    /// blocks until the test decides what to do with it.
    fn spawn_run(&self, prompt: &str, extra: &[&str]) -> Child {
        let mut command = self.base_command();
        command
            .args(["run", "--message", prompt, "--no-stream"])
            .args(extra)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.spawn().expect("phi-headless should start")
    }

    fn run(&self, prompt: &str, extra: &[&str]) -> Value {
        let mut args = vec!["run", "--message", prompt, "--no-stream"];
        args.extend_from_slice(extra);
        let output = self.command(&args);
        assert!(
            output.status.success(),
            "run failed: status={:?}\nstdout={}\nstderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        last_json(&output.stdout)
    }

    fn show(&self, session_id: &str) -> Value {
        let output = self.command(&["sessions", "show", session_id]);
        assert!(
            output.status.success(),
            "sessions show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        last_json(&output.stdout)
    }

    /// Poll the journal through read-only shells until the latest run reaches
    /// `wanted`. Read-only commands never take the writer lock, so this is
    /// safe while the run process is still alive.
    fn wait_for_run_state(&self, wanted: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut last_state = String::from("<none>");
        while Instant::now() < deadline {
            if let Some(list) = try_last_json(&self.command(&["sessions", "list"]).stdout) {
                if let Some(id) = list["sessions"]
                    .as_array()
                    .and_then(|sessions| sessions.first())
                    .and_then(|session| session["id"].as_str())
                {
                    if let Some(detail) =
                        try_last_json(&self.command(&["sessions", "show", id]).stdout)
                    {
                        if let Some(run) = detail["session"]["runs"]
                            .as_array()
                            .and_then(|runs| runs.last())
                        {
                            last_state = run["state"].as_str().unwrap_or("").to_string();
                            if last_state == wanted {
                                return run.clone();
                            }
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("run did not reach {wanted}; last state: {last_state}");
    }
}

fn last_json(stdout: &[u8]) -> Value {
    try_last_json(stdout).unwrap_or_else(|| {
        let text = String::from_utf8_lossy(stdout);
        panic!("no JSON line in stdout: {text}")
    })
}

fn try_last_json(stdout: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(stdout);
    let line = text
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))?;
    serde_json::from_str(line).ok()
}

/// Every tool result recorded in the session, joined for substring checks.
fn tool_result_text(session: &Value) -> String {
    let mut out = String::new();
    for message in session["session"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
    {
        for block in message["blocks"].as_array().into_iter().flatten() {
            if block["type"] == "tool_result" {
                if let Some(content) = block["content"].as_str() {
                    out.push_str(content);
                    out.push('\n');
                }
            }
        }
    }
    out
}

#[test]
fn workbench_run_updates_the_session_view() {
    let env = TestEnv::new("workbench");
    let result = env.run("show me a small dashboard", &["--workbench"]);
    assert_eq!(result["run"]["state"], "Succeeded");
    assert_eq!(result["run"]["workbenchMode"], json!(true));

    let session = env.show(result["sessionId"].as_str().unwrap());
    let view = &session["session"]["uiView"];
    assert_eq!(view["version"], json!(1));
    assert_eq!(view["root"]["kind"], "stack");
    // The mock model's view is a full tree, not an opaque program string.
    assert!(view["root"]["children"].as_array().unwrap().len() >= 2);
}

#[test]
fn write_approval_allow_creates_the_file() {
    let env = TestEnv::new("allow");
    env.run("demo write", &["--approval", "allow"]);
    let path = env.workspace.join("PHI_DEMO.md");
    assert!(path.is_file(), "approved write should create {path:?}");
    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("# phi-agent demo"));
}

#[test]
fn write_approval_deny_leaves_the_workspace_untouched() {
    let env = TestEnv::new("deny");
    env.run("demo write", &["--approval", "deny"]);
    assert!(
        !env.workspace.join("PHI_DEMO.md").exists(),
        "denied write must not touch the workspace"
    );
}

#[test]
fn sessions_and_journal_survive_a_restart() {
    let env = TestEnv::new("restart");
    let result = env.run("inspect the workspace", &["--approval", "deny"]);
    let session_id = result["sessionId"].as_str().unwrap().to_string();

    // A fresh process (fresh Node composition) must rebuild the same session.
    let session = env.show(&session_id);
    assert_eq!(session["session"]["id"].as_str(), Some(session_id.as_str()));
    assert!(
        !session["session"]["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn status_reads_a_finished_run_from_a_different_process() {
    let env = TestEnv::new("status");
    let result = env.run("inspect the workspace", &["--approval", "deny"]);
    let run_id = result["runId"].as_str().unwrap();

    let output = env.command(&["status", run_id]);
    assert!(
        output.status.success(),
        "status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = last_json(&output.stdout);
    assert_eq!(status["found"], json!(true));
    assert_eq!(status["status"]["state"], "Succeeded");
}

#[test]
fn step_limit_aborts_the_run() {
    let env = TestEnv::new("steps");
    let output = env.command(&[
        "run",
        "--message",
        "inspect the workspace",
        "--no-stream",
        "--max-steps",
        "1",
        "--approval",
        "deny",
    ]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "expected aborted exit code, stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = last_json(&output.stdout);
    assert_eq!(result["run"]["state"], "Aborted");
}

#[test]
fn cost_budget_aborts_before_the_model_call() {
    let env = TestEnv::new("budget");
    // A priced provider with a literal key makes `estimate_cost` non-zero;
    // the tiny cap must fail the pre-call reservation, so no request is sent.
    let add = env.command(&[
        "providers",
        "add",
        "--name",
        "Pricey",
        "--protocol",
        "openai_chat",
        "--base-url",
        "https://example.invalid/v1",
        "--api-key",
        "literal:test-key",
        "--model",
        "test-model",
        "--pricing-in",
        "1000",
        "--pricing-out",
        "1000",
        "--activate",
    ]);
    assert!(
        add.status.success(),
        "providers add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let output = env.command(&[
        "run",
        "--message",
        "inspect the workspace",
        "--no-stream",
        "--approval",
        "deny",
        "--max-cost",
        "0.000001",
    ]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "expected aborted exit code, stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = last_json(&output.stdout);
    assert_eq!(result["run"]["state"], "Aborted");
    let error = result["run"]["error"].as_str().unwrap_or("");
    assert!(
        error.contains("Cost budget exceeded"),
        "unexpected error: {error}"
    );

    // Only the user message exists: the model was never called.
    let session = env.show(result["sessionId"].as_str().unwrap());
    assert_eq!(
        session["session"]["messages"].as_array().unwrap().len(),
        1,
        "budget abort must happen before the model call"
    );
}

#[test]
fn cancel_while_waiting_for_approval_aborts_the_run() {
    let env = TestEnv::new("cancel");
    let mut child = env.spawn_run("demo write", &["--approval", "prompt"]);
    let run = env.wait_for_run_state("WaitingApproval");
    let run_id = run["id"].as_str().unwrap().to_string();

    let cancel = env.command(&["cancel", &run_id]);
    assert!(
        cancel.status.success(),
        "cancel failed: {}",
        String::from_utf8_lossy(&cancel.stderr)
    );

    let aborted = env.wait_for_run_state("Aborted");
    assert_eq!(aborted["error"], "Cancelled by user");
    assert!(
        aborted["stopConfirmation"]
            .as_str()
            .is_some_and(|stop| !stop.is_empty()),
        "cancelled runs must carry a stop confirmation"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_killed_run_is_marked_for_reconciliation_on_restart() {
    let env = TestEnv::new("crash");
    let mut child = env.spawn_run("demo write", &["--approval", "prompt"]);
    let run = env.wait_for_run_state("WaitingApproval");
    let run_id = run["id"].as_str().unwrap().to_string();

    // Simulate a crash: no terminal event, no graceful shutdown.
    child.kill().unwrap();
    let _ = child.wait();

    // Any writable composition performs crash recovery; bootstrap is the
    // cheapest one that also reports the repaired session list.
    let bootstrap = env.command(&["bootstrap"]);
    assert!(
        bootstrap.status.success(),
        "bootstrap after crash failed: {}",
        String::from_utf8_lossy(&bootstrap.stderr)
    );

    let output = env.command(&["status", &run_id]);
    assert!(
        output.status.success(),
        "status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = last_json(&output.stdout);
    assert_eq!(status["found"], json!(true));
    assert_eq!(status["status"]["state"], "AwaitingReconciliation");

    // The session projection carries the human-readable note, and the run is
    // never retried automatically.
    let list = last_json(&env.command(&["sessions", "list"]).stdout);
    let session_id = list["sessions"][0]["id"].as_str().unwrap();
    let session = env.show(session_id);
    let runs = session["session"]["runs"].as_array().unwrap();
    assert_eq!(
        runs.len(),
        1,
        "a crashed run must not be retried automatically"
    );
    assert_eq!(runs[0]["state"], "AwaitingReconciliation");
    let note = runs[0]["error"].as_str().unwrap_or("");
    assert!(
        note.contains("approval was pending"),
        "unexpected reconciliation note: {note}"
    );
}

#[test]
fn shell_exec_runs_only_after_approval() {
    let env = TestEnv::new("shell-allow");
    let result = env.run("demo shell", &["--approval", "allow"]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        results.contains("phi-agent-shell-demo"),
        "approved shell command output missing: {results}"
    );
}

#[test]
fn denying_shell_exec_does_not_run_the_command() {
    let env = TestEnv::new("shell-deny");
    let result = env.run("demo shell", &["--approval", "deny"]);
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        !results.contains("phi-agent-shell-demo"),
        "denied shell command must not run: {results}"
    );
    assert!(
        results.to_lowercase().contains("denied"),
        "denial should be recorded: {results}"
    );
}

#[test]
fn legacy_provider_files_still_load() {
    let env = TestEnv::new("legacy-providers");
    // The shape written by earlier builds: string models and the old
    // `open_ai_chat` protocol id.
    std::fs::write(
        env.data_dir.join("providers.json"),
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "activeId": "legacy",
            "providers": [{
                "id": "legacy",
                "name": "Legacy",
                "protocol": "open_ai_chat",
                "baseUrl": "https://example.com/v1",
                "apiKey": "env:PHI_LEGACY_KEY",
                "model": "claude-sonnet-4.5",
                "models": ["claude-sonnet-4.5", "gpt-5-mini"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let output = env.command(&["providers", "list"]);
    assert!(
        output.status.success(),
        "providers list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let list = last_json(&output.stdout);
    assert_eq!(list["activeProviderId"], "legacy");
    let provider = &list["providers"][0];
    assert_eq!(provider["protocol"], "openai_chat");
    let models = provider["models"].as_array().unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0]["id"], "claude-sonnet-4.5");
    assert_eq!(models[0]["name"], "claude-sonnet-4.5");
}

#[test]
fn provider_add_fills_catalogue_metadata_and_picks_a_primary_model() {
    let env = TestEnv::new("provider-add");
    let output = env.command(&[
        "providers",
        "add",
        "--name",
        "Catalogue",
        "--protocol",
        "openai_chat",
        "--base-url",
        "https://example.com/v1",
        "--api-key",
        "literal:test-key",
        "--models",
        "claude-sonnet-4.5,gpt-5-mini",
        "--activate",
    ]);
    assert!(
        output.status.success(),
        "providers add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let view = last_json(&output.stdout);
    assert_eq!(view["model"], "claude-sonnet-4.5");
    let models = view["models"].as_array().unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0]["name"], "Claude Sonnet");
    assert_eq!(models[1]["name"], "GPT-5 mini");
}
