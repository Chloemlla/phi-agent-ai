//! Headless smoke tests.
//!
//! These run the real `phi-headless` binary, which is also how the desktop
//! shell is exercised in CI. Each test points the node at a scripted local
//! Anthropic-compatible HTTP server, so the real protocol path is exercised
//! without network access or a product-side mock.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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

    fn memory_add(&self, fact: &str, scope: &str) -> Value {
        let output = self.command(&["memory", "add", "--fact", fact, "--scope", scope]);
        assert!(
            output.status.success(),
            "memory add failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        last_json(&output.stdout)
    }

    fn memory_search(&self, query: &str) -> Value {
        let output = self.command(&["memory", "search", query]);
        assert!(
            output.status.success(),
            "memory search failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        last_json(&output.stdout)
    }

    fn memory_list(&self, text: Option<&str>) -> Vec<Value> {
        let mut args = vec!["memory", "list"];
        if let Some(text) = text {
            args.extend_from_slice(&["--text", text]);
        }
        let output = self.command(&args);
        assert!(
            output.status.success(),
            "memory list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        last_json(&output.stdout)["memories"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// Point the node at a deterministic local model endpoint. The provider
    /// file is written directly so every run uses the real wire protocol.
    fn with_scripted_provider(&self) -> ScriptedProvider {
        let provider = ScriptedProvider::start();
        let file = json!({
            "version": 1,
            "activeId": "scripted",
            "providers": [{
                "id": "scripted",
                "name": "Scripted provider",
                "protocol": "anthropic_messages",
                "baseUrl": format!("http://127.0.0.1:{}", provider.port),
                "auth": "none",
                "model": "test-model",
                "models": [
                    { "id": "test-model", "scale": 5 },
                    { "id": "small-model", "scale": 2, "name": "Small model" }
                ]
            }]
        });
        std::fs::write(
            self.data_dir.join("providers.json"),
            serde_json::to_vec_pretty(&file).unwrap(),
        )
        .unwrap();
        provider
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

/// A local Anthropic Messages endpoint with a scripted policy: the request
/// history decides which tool to call, and a tool result always ends the run.
struct ScriptedProvider {
    port: u16,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl ScriptedProvider {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind scripted provider");
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorder = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(stream) = stream {
                    let _ = handle_model_request(stream, recorder.clone());
                }
            }
        });
        Self { port, requests }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
}

fn handle_model_request(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<Value>>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if let Ok(mut log) = requests.lock() {
        log.push(request.clone());
    }
    let payload = serde_json::to_vec(&scripted_response(&request)).unwrap();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(&payload)?;
    stream.flush()
}

static TOOL_CALL_SEQ: AtomicU64 = AtomicU64::new(1);

fn scripted_response(request: &Value) -> Value {
    let mut last_user_text = String::new();
    let mut last_tool_result = None;
    let mut tool_use_count = 0usize;
    for message in request["messages"].as_array().into_iter().flatten() {
        for block in message["content"].as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("tool_use") => tool_use_count += 1,
                Some("text") if message["role"] == "user" => {
                    if let Some(text) = block["text"].as_str() {
                        last_user_text = text.to_string();
                    }
                }
                Some("tool_result") => {
                    last_tool_result = block["content"].as_str().map(str::to_string);
                }
                _ => {}
            }
        }
    }

    let lower = last_user_text.to_lowercase();
    let next_id = || format!("toolu_{}", TOOL_CALL_SEQ.fetch_add(1, Ordering::Relaxed));

    // The subagent thread receives a brief; answer it with a plain report so
    // the delegation path can be tested without a workspace side effect.
    if lower.contains("you are a subagent") || lower.contains("brief from the lead agent") {
        return message_response(
            vec![json!({
                "type": "text",
                "text": "Subagent report: inspected the workspace and finished the brief."
            })],
            "end_turn",
        );
    }

    // Session-title auxiliary call: the prompt starts with the first
    // user/assistant exchange.
    if lower.starts_with("user:") && lower.contains("assistant:") {
        return message_response(
            vec![json!({ "type": "text", "text": "Workspace inspection" })],
            "end_turn",
        );
    }

    // Memory-maintenance auxiliary call: merge the first two facts.
    if lower.contains("active facts:") {
        let value = maintenance_merge_response(&last_user_text);
        return message_response(vec![json!({ "type": "text", "text": value })], "end_turn");
    }

    if lower.contains("draw an image") && last_tool_result.is_none() {
        return message_response(
            vec![json!({
                "type": "tool_use",
                "id": next_id(),
                "name": "image_generate",
                "input": { "prompt": "a red cube on a table", "count": 1 }
            })],
            "tool_use",
        );
    }

    if lower.contains("read the large file") && last_tool_result.is_none() {
        return message_response(
            vec![json!({
                "type": "tool_use",
                "id": next_id(),
                "name": "fs_read",
                "input": { "path": "LARGE.txt" }
            })],
            "tool_use",
        );
    }

    if let Some(result) = last_tool_result.as_deref() {
        if result.contains("\"timedOut\":false") {
            return message_response(
                vec![
                    json!({ "type": "text", "text": "Lead done after collecting the subagent report." }),
                ],
                "end_turn",
            );
        }
        if result.contains("more capable") {
            return message_response(
                vec![json!({ "type": "text", "text": "Model cap enforced." })],
                "end_turn",
            );
        }
    }

    if lower.contains("too capable") {
        return message_response(
            vec![json!({
                "type": "tool_use",
                "id": next_id(),
                "name": "delegate",
                "input": {
                    "model": "test-model",
                    "work_mode": "read_only",
                    "brief": { "objective": "This model is larger than the lead" }
                }
            })],
            "tool_use",
        );
    }

    if lower.contains("delegate") || lower.contains("active subagents") {
        if let Some(agent_id) = last_tool_result.as_deref().and_then(extract_agent_id) {
            return message_response(
                vec![json!({
                    "type": "tool_use",
                    "id": next_id(),
                    "name": "wait_for_agents",
                    "input": { "agent_ids": [agent_id], "mode": "all", "timeout_secs": 60 }
                })],
                "tool_use",
            );
        }
        return message_response(
            vec![json!({
                "type": "tool_use",
                "id": next_id(),
                "name": "delegate",
                "input": {
                    "model": "small-model",
                    "work_mode": "read_only",
                    "wait": false,
                    "brief": {
                        "objective": "Inspect the workspace and produce a short report",
                        "acceptance": "A report naming what was inspected"
                    }
                }
            })],
            "tool_use",
        );
    }

    if lower.contains("truncate forever") {
        // Every request is cut off with a tool call, so the bounded recovery
        // limit is exercised rather than a one-off truncation.
        return message_response(
            vec![
                json!({ "type": "text", "text": "Still working." }),
                json!({
                    "type": "tool_use",
                    "id": next_id(),
                    "name": "fs_list",
                    "input": { "path": "." }
                }),
            ],
            "length",
        );
    }

    if lower.contains("cut off by the output token limit") {
        // The text-only continuation is injected into the next request but is
        // not stored as a user message.
        return message_response(
            vec![json!({ "type": "text", "text": "Completed the rest." })],
            "end_turn",
        );
    }

    if lower.contains("truncate text") {
        return message_response(
            vec![json!({ "type": "text", "text": "Partial answer." })],
            "length",
        );
    }

    if lower.contains("truncate") && last_tool_result.is_none() {
        return message_response(
            vec![
                json!({ "type": "text", "text": "Working on it." }),
                json!({
                    "type": "tool_use",
                    "id": next_id(),
                    "name": "fs_write",
                    "input": {
                        "path": "PHI_TRUNCATED.md",
                        "content": "This must never be written."
                    }
                }),
            ],
            "length",
        );
    }

    if lower.contains("many steps") || lower.contains("keep working") {
        if tool_use_count < 25 {
            return message_response(
                vec![json!({
                    "type": "tool_use",
                    "id": next_id(),
                    "name": "fs_list",
                    "input": { "path": "." }
                })],
                "tool_use",
            );
        }
        return message_response(
            vec![json!({ "type": "text", "text": "Done after many steps." })],
            "end_turn",
        );
    }

    if let Some(result) = last_tool_result {
        let summary: String = result.chars().take(200).collect();
        return message_response(
            vec![json!({ "type": "text", "text": format!("Done. {summary}") })],
            "end_turn",
        );
    }

    if lower.contains("dashboard") {
        // Inline interface blocks are ordinary assistant text: the node must
        // journal the HTML fence exactly as the model produced it.
        return message_response(
            vec![json!({
                "type": "text",
                "text": "Here is the layout.\n\n```html\n<section>\n  <h2>Plan</h2>\n  <ol><li>Read</li><li>Change</li></ol>\n</section>\n```\n"
            })],
            "end_turn",
        );
    }

    if lower.contains("unsafe shell") {
        // Not on the safe whitelist, so this call must go through approval (or
        // decision triage when configured).
        let command = if cfg!(windows) { "ver" } else { "uname -s" };
        return message_response(
            vec![json!({
                "type": "tool_use",
                "id": next_id(),
                "name": "shell_exec",
                "input": { "command": command, "timeout_secs": 30 }
            })],
            "tool_use",
        );
    }

    let (name, input) = if lower.contains("write") {
        (
            "fs_write",
            json!({
                "path": "PHI_DEMO.md",
                "content": "# phi-agent demo\n\nWritten for the approval test.\n"
            }),
        )
    } else if lower.contains("shell") || lower.contains("echo") {
        (
            "shell_exec",
            json!({ "command": "echo phi-agent-shell-demo", "timeout_secs": 30 }),
        )
    } else if lower.contains("remember") {
        (
            "memory_write",
            json!({ "fact": "The user prefers dark mode", "scope": "user" }),
        )
    } else if lower.contains("recall") {
        ("memory_search", json!({ "query": "deploy target" }))
    } else {
        ("fs_list", json!({ "path": "." }))
    };
    message_response(
        vec![
            json!({ "type": "text", "text": "Working on it." }),
            json!({ "type": "tool_use", "id": next_id(), "name": name, "input": input }),
        ],
        "tool_use",
    )
}

fn extract_agent_id(result: &str) -> Option<String> {
    let value: Value = serde_json::from_str(result).ok()?;
    value
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn maintenance_merge_response(user: &str) -> String {
    let Some(start) = user.find('[') else {
        return json!({ "operations": [] }).to_string();
    };
    let Some(end) = user.rfind(']') else {
        return json!({ "operations": [] }).to_string();
    };
    let facts: Vec<Value> = serde_json::from_str(&user[start..=end]).unwrap_or_default();
    let ids: Vec<&str> = facts
        .iter()
        .filter_map(|fact| fact["id"].as_str())
        .take(2)
        .collect();
    if ids.len() < 2 {
        return json!({ "operations": [] }).to_string();
    }
    let fact = format!(
        "Merged preference: {}; {}",
        facts[0]["fact"].as_str().unwrap_or_default(),
        facts[1]["fact"].as_str().unwrap_or_default()
    );
    json!({
        "operations": [{
            "op": "merge",
            "ids": ids,
            "fact": fact,
            "reason": "the two facts describe the same preference"
        }]
    })
    .to_string()
}

fn message_response(content: Vec<Value>, stop_reason: &str) -> Value {
    json!({
        "id": "msg_scripted",
        "type": "message",
        "role": "assistant",
        "model": "test-model",
        "content": content,
        "stop_reason": stop_reason,
        "usage": { "input_tokens": 12, "output_tokens": 6 }
    })
}

/// A local OpenAI Chat Completions endpoint that first returns assistant text
/// plus a tool call with malformed JSON arguments, then a final answer once it
/// sees the paired tool result. Used to prove the malformed-arguments
/// regression end to end.
struct MalformedArgsProvider {
    port: u16,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl MalformedArgsProvider {
    fn start() -> Self {
        Self::with_recovery(true)
    }

    fn always_malformed() -> Self {
        Self::with_recovery(false)
    }

    fn with_recovery(recover: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind malformed provider");
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorder = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(stream) = stream {
                    let _ = handle_malformed_request(stream, recorder.clone(), recover);
                }
            }
        });
        Self { port, requests }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
}

fn handle_malformed_request(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<Value>>>,
    recover: bool,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if let Ok(mut log) = requests.lock() {
        log.push(request.clone());
    }

    let saw_tool_result = request["messages"]
        .as_array()
        .is_some_and(|messages| messages.iter().any(|message| message["role"] == "tool"));
    let response = if recover && saw_tool_result {
        json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "The tool call was rejected, so I recovered without touching the file."
                },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 30, "completion_tokens": 8 }
        })
    } else {
        json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "I will inspect that file first.",
                    "tool_calls": [{
                        "id": "call_bad",
                        "type": "function",
                        "function": {
                            "name": "fs_write",
                            "arguments": "{\"path\": src/main.rs, \"content\": \"must not run\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 20, "completion_tokens": 10 }
        })
    };
    let payload = serde_json::to_vec(&response).unwrap();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(&payload)?;
    stream.flush()
}

#[test]
fn inline_html_replies_are_journaled_and_prompted() {
    let env = TestEnv::new("inline-ui");
    let provider = env.with_scripted_provider();
    let result = env.run("show me a dashboard", &[]);
    assert_eq!(result["run"]["state"], "Succeeded");

    // Every model call carries the inline interface guidance.
    let requests = provider.requests();
    let system = requests[0]["system"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        system.contains("# Inline interface blocks"),
        "system prompt is missing the inline UI guidance"
    );

    // The reply, including its HTML fence, is stored as ordinary text.
    let session = env.show(result["sessionId"].as_str().unwrap());
    let text: String = session["session"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|message| message["role"] == "assistant")
        .flat_map(|message| message["blocks"].as_array().into_iter().flatten())
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("```html"), "assistant text lost the fence");
    assert!(text.contains("<section>"), "assistant text lost the HTML");
}

#[test]
fn workspace_writes_run_without_approval() {
    let env = TestEnv::new("write-auto");
    let _provider = env.with_scripted_provider();
    // Even --approval deny must not block a workspace write: the default
    // policy allows it and the previous content is backed up.
    let result = env.run("write the demo file", &["--approval", "deny"]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let path = env.workspace.join("PHI_DEMO.md");
    assert!(path.is_file(), "workspace write should create {path:?}");
    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("# phi-agent demo"));
}

#[test]
fn sessions_and_journal_survive_a_restart() {
    let env = TestEnv::new("restart");
    let _provider = env.with_scripted_provider();
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
fn sessions_delete_removes_the_session_from_the_list() {
    let env = TestEnv::new("session-delete");
    let _provider = env.with_scripted_provider();
    let result = env.run("inspect the workspace", &["--approval", "deny"]);
    let session_id = result["sessionId"].as_str().unwrap().to_string();

    let output = env.command(&["sessions", "delete", &session_id]);
    assert!(
        output.status.success(),
        "sessions delete failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let deleted = last_json(&output.stdout);
    assert_eq!(deleted["deleted"], json!(true));

    let list = last_json(&env.command(&["sessions", "list"]).stdout);
    assert!(
        list["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|session| session["id"] != session_id.as_str()),
        "deleted session still listed: {list}"
    );
    let show = env.command(&["sessions", "show", &session_id]);
    assert!(
        !show.status.success(),
        "deleted session should no longer be readable"
    );
}

#[test]
fn status_reads_a_finished_run_from_a_different_process() {
    let env = TestEnv::new("status");
    let _provider = env.with_scripted_provider();
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
fn run_is_not_capped_at_twenty_steps() {
    let env = TestEnv::new("many-steps");
    let _provider = env.with_scripted_provider();
    let result = env.run(
        "keep working until the task is done",
        &["--approval", "deny"],
    );
    assert_eq!(result["run"]["state"], "Succeeded");
    assert!(
        result["run"]["steps"].as_u64().unwrap() >= 25,
        "the run must continue past the old 20-step default: {}",
        result["run"]["steps"]
    );
    assert!(result["run"]["inputTokens"].as_u64().unwrap() > 0);
    assert!(result["run"]["outputTokens"].as_u64().unwrap() > 0);
    assert!(
        result["run"].get("costUsd").is_none(),
        "pricing was removed; runs only record tokens"
    );
}

#[test]
fn run_forwards_model_and_thinking_to_the_provider() {
    let env = TestEnv::new("thinking");
    let provider = env.with_scripted_provider();
    let result = env.run(
        "inspect the workspace",
        &[
            "--approval",
            "deny",
            "--model",
            "test-model",
            "--thinking",
            "high",
        ],
    );
    assert_eq!(result["run"]["state"], "Succeeded");

    let requests = provider.requests();
    let first = &requests[0];
    assert_eq!(first["model"], "test-model");
    assert_eq!(first["thinking"]["type"], "adaptive");
    assert_eq!(first["output_config"]["effort"], "high");
    assert!(
        first.get("temperature").is_none(),
        "current Anthropic models reject temperature"
    );

    for level in ["xhigh", "max"] {
        let result = env.run(
            "inspect the workspace",
            &[
                "--approval",
                "deny",
                "--model",
                "test-model",
                "--thinking",
                level,
            ],
        );
        assert_eq!(result["run"]["state"], "Succeeded", "{level}: {result}");
        assert!(
            provider
                .requests()
                .iter()
                .any(|request| request["output_config"]["effort"] == level),
            "the {level} effort must reach the provider"
        );
    }

    let output = env.command(&[
        "run",
        "--message",
        "inspect the workspace",
        "--no-stream",
        "--thinking",
        "extreme",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Unknown thinking effort"),
        "an unknown effort must fail clearly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn truncated_tool_call_is_not_executed_and_the_run_recovers() {
    let env = TestEnv::new("truncated");
    let _provider = env.with_scripted_provider();
    let result = env.run("truncate the response", &["--approval", "deny"]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        results.contains("truncated"),
        "the truncated tool call must be reported: {results}"
    );
    assert!(
        !env.workspace.join("PHI_TRUNCATED.md").exists(),
        "a truncated tool call must not be executed"
    );
    let transcript = serde_json::to_string(&session["session"]["messages"]).unwrap();
    assert!(
        transcript.contains("Working on it."),
        "the partial assistant text must be kept: {transcript}"
    );
}

#[test]
fn truncation_recovery_is_capped() {
    let env = TestEnv::new("truncate-cap");
    let _provider = env.with_scripted_provider();
    let output = env.command(&[
        "run",
        "--message",
        "truncate forever",
        "--no-stream",
        "--approval",
        "deny",
    ]);
    assert_eq!(
        output.status.code(),
        Some(4),
        "the run must eventually fail honestly, stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = last_json(&output.stdout);
    assert_eq!(result["run"]["state"], "Failed");
    let error = result["run"]["error"].as_str().unwrap_or("");
    assert!(
        error.contains("truncated") && error.contains("three times"),
        "the failure must name the limit: {error}"
    );
    assert!(
        error.contains("8192"),
        "the failure must name the output token limit: {error}"
    );
}

#[test]
fn text_truncation_continuation_is_ephemeral() {
    let env = TestEnv::new("truncate-text");
    let _provider = env.with_scripted_provider();
    let result = env.run("truncate text", &["--approval", "deny"]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let session = env.show(result["sessionId"].as_str().unwrap());
    let transcript = serde_json::to_string(&session["session"]["messages"]).unwrap();
    assert!(
        transcript.contains("Partial answer."),
        "the partial answer must be persisted: {transcript}"
    );
    assert!(
        transcript.contains("Completed the rest."),
        "the continuation must complete the answer: {transcript}"
    );
    assert!(
        !transcript.contains("Continue exactly where you stopped"),
        "the continuation instruction must not appear as a human message: {transcript}"
    );
}

#[test]
fn malformed_tool_arguments_keep_text_and_do_not_execute_the_tool() {
    let env = TestEnv::new("malformed-args");
    let provider = MalformedArgsProvider::start();
    std::fs::write(
        env.data_dir.join("providers.json"),
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "activeId": "scripted",
            "providers": [{
                "id": "scripted",
                "name": "Malformed provider",
                "protocol": "openai_chat",
                "baseUrl": format!("http://127.0.0.1:{}", provider.port),
                "auth": "none",
                "model": "test-model",
                "models": [{ "id": "test-model" }]
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let result = env.run("inspect the file please", &["--approval", "deny"]);
    assert_eq!(result["run"]["state"], "Succeeded");

    let session = env.show(result["sessionId"].as_str().unwrap());
    let transcript = serde_json::to_string(&session["session"]["messages"]).unwrap();
    assert!(
        transcript.contains("I will inspect that file first."),
        "the assistant text before the malformed call must be kept: {transcript}"
    );
    assert!(
        transcript.contains("The tool call was rejected"),
        "the run must continue after the malformed call: {transcript}"
    );

    let results = tool_result_text(&session);
    assert!(
        results.contains("not valid JSON"),
        "the tool result must explain the parse failure: {results}"
    );
    assert!(
        results.contains("fs_write"),
        "the tool result must name the tool: {results}"
    );

    assert!(
        !env.workspace.join("src").exists(),
        "a malformed tool call must never execute"
    );
    assert_eq!(
        provider.requests().len(),
        2,
        "one malformed call and one recovery request"
    );
}

#[test]
fn endlessly_malformed_tool_arguments_stop_the_run() {
    let env = TestEnv::new("malformed-cap");
    let provider = MalformedArgsProvider::always_malformed();
    std::fs::write(
        env.data_dir.join("providers.json"),
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "activeId": "scripted",
            "providers": [{
                "id": "scripted",
                "name": "Malformed provider",
                "protocol": "openai_chat",
                "baseUrl": format!("http://127.0.0.1:{}", provider.port),
                "auth": "none",
                "model": "test-model",
                "models": [{ "id": "test-model" }]
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let output = env.command(&[
        "run",
        "--message",
        "keep trying",
        "--no-stream",
        "--approval",
        "deny",
    ]);
    assert_eq!(
        output.status.code(),
        Some(4),
        "the run must eventually fail honestly, stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = last_json(&output.stdout);
    assert_eq!(result["run"]["state"], "Failed");
    let error = result["run"]["error"].as_str().unwrap_or("");
    assert!(
        error.contains("repeatedly sent tool arguments"),
        "the failure must explain the repeated invalid calls: {error}"
    );
    assert_eq!(
        provider.requests().len(),
        5,
        "the loop must stop after the bounded number of recoveries"
    );
    assert!(
        !env.workspace.join("src").exists(),
        "a malformed tool call must never execute"
    );
}

#[test]
fn cancel_while_waiting_for_approval_aborts_the_run() {
    let env = TestEnv::new("cancel");
    let _provider = env.with_scripted_provider();
    let mut child = env.spawn_run("run an unsafe shell command", &["--approval", "prompt"]);
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
    let _provider = env.with_scripted_provider();
    let mut child = env.spawn_run("run an unsafe shell command", &["--approval", "prompt"]);
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
fn unsafe_shell_runs_after_approval() {
    let env = TestEnv::new("shell-allow");
    let _provider = env.with_scripted_provider();
    let result = env.run("run an unsafe shell command", &["--approval", "allow"]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        results.contains("Windows") || results.contains("Linux") || results.contains("Darwin"),
        "approved shell command output missing: {results}"
    );
}

#[test]
fn denying_unsafe_shell_does_not_run_the_command() {
    let env = TestEnv::new("shell-deny");
    let _provider = env.with_scripted_provider();
    let result = env.run("run an unsafe shell command", &["--approval", "deny"]);
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        !results.contains("Windows") && !results.contains("Linux") && !results.contains("Darwin"),
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
fn provider_add_keeps_unknown_models_manual_and_picks_a_primary() {
    let env = TestEnv::new("provider-add");
    let output = env.command(&[
        "providers",
        "add",
        "--name",
        "Manual",
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
    // The stale hardcoded catalogue is gone; unknown ids stay manual until
    // the user (or a fetched catalogue) supplies metadata.
    assert_eq!(models[0]["name"], "claude-sonnet-4.5");
    assert_eq!(models[0]["source"], "manual");
    assert_eq!(models[1]["name"], "gpt-5-mini");
    assert_eq!(models[1]["source"], "manual");
}

#[test]
fn memory_is_injected_into_the_system_prompt() {
    let env = TestEnv::new("memory-inject");
    let provider = env.with_scripted_provider();
    env.memory_add("The user prefers dark mode in every editor", "user");
    let result = env.run("which editor theme does the user prefer", &[]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let requests = provider.requests();
    assert!(!requests.is_empty());
    let first = serde_json::to_string(&requests[0]).unwrap();
    assert!(first.contains("Long-term memory"), "system prompt: {first}");
    assert!(first.contains("dark mode"), "system prompt: {first}");
}

#[test]
fn agent_can_recall_memory_with_the_tool() {
    let env = TestEnv::new("memory-recall");
    env.with_scripted_provider();
    env.memory_add("The deploy target is fly.io", "project");
    let hits = env.memory_search("deploy target");
    assert_eq!(hits["hits"].as_array().unwrap().len(), 1);
    let result = env.run("please recall the deployment target", &[]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let session = env.show(result["sessionId"].as_str().unwrap());
    let text = tool_result_text(&session);
    assert!(text.contains("fly.io"), "tool result: {text}");
    assert!(text.contains("unverified data"), "tool result: {text}");
}

#[test]
fn agent_can_write_memory_with_the_tool() {
    let env = TestEnv::new("memory-write");
    env.with_scripted_provider();
    let result = env.run("remember that the user prefers dark mode", &[]);
    assert_eq!(result["run"]["state"], "Succeeded");
    let memories = env.memory_list(Some("dark mode"));
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["sourceTrust"], "model");
    assert_eq!(memories[0]["scope"], "user");
    assert_eq!(memories[0]["state"], "active");
}

#[test]
fn explicit_workspace_scopes_memory_queries() {
    let env = TestEnv::new("memory-workspace");
    env.memory_add("The deploy target is fly.io", "project");
    let other = std::env::temp_dir().join(format!("phi-other-ws-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&other).unwrap();
    let output = env.command(&[
        "--workspace",
        other.to_str().unwrap(),
        "memory",
        "search",
        "deploy target",
    ]);
    assert!(
        output.status.success(),
        "search failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = last_json(&output.stdout);
    assert!(
        result["hits"].as_array().unwrap().is_empty(),
        "project memory must follow the explicit workspace: {result}"
    );
    let _ = std::fs::remove_dir_all(other);
}

#[test]
fn memory_reads_degrade_on_a_fresh_data_dir() {
    let env = TestEnv::new("memory-fresh");
    let list = env.command(&["memory", "list"]);
    assert!(
        list.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    assert!(
        last_json(&list.stdout)["memories"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let stats = env.command(&["memory", "stats"]);
    assert!(stats.status.success());
    assert_eq!(last_json(&stats.stdout)["stats"]["total"], 0);

    let search = env.command(&["memory", "search", "anything"]);
    assert!(search.status.success());
    assert!(
        last_json(&search.stdout)["hits"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn memory_cli_forgets_and_rejects_secrets() {
    let env = TestEnv::new("memory-cli");
    let added = env.memory_add("The user prefers tabs", "user");
    let id = added["recordId"].as_str().unwrap().to_string();

    let secret = env.command(&[
        "memory",
        "add",
        "--fact",
        "sk-test0000000000000000",
        "--scope",
        "user",
    ]);
    assert_eq!(secret.status.code(), Some(4));
    let rejected = last_json(&secret.stdout);
    assert_eq!(rejected["reason"], "secret");

    let run_scope = env.command(&[
        "memory",
        "add",
        "--fact",
        "only the agent writes run memories",
        "--scope",
        "run",
    ]);
    assert_eq!(run_scope.status.code(), Some(2));

    let bare_forget = env.command(&["memory", "forget"]);
    assert_eq!(bare_forget.status.code(), Some(2));

    let nan_confidence = env.command(&[
        "memory",
        "add",
        "--fact",
        "uses pnpm",
        "--scope",
        "user",
        "--confidence",
        "NaN",
    ]);
    assert_eq!(nan_confidence.status.code(), Some(2));

    let flag_invalidate = env.command(&[
        "memory",
        "invalidate",
        "--id",
        id.as_str(),
        "--reason",
        "flag form",
    ]);
    assert!(
        flag_invalidate.status.success(),
        "flag-form invalidate failed: {}",
        String::from_utf8_lossy(&flag_invalidate.stderr)
    );
    let shown = env.command(&["memory", "show", id.as_str()]);
    assert!(shown.status.success());
    assert_eq!(last_json(&shown.stdout)["record"]["state"], "invalidated");

    let forget = env.command(&[
        "memory",
        "forget",
        "--id",
        id.as_str(),
        "--reason",
        "test cleanup",
    ]);
    assert!(
        forget.status.success(),
        "forget failed: {}",
        String::from_utf8_lossy(&forget.stderr)
    );
    assert!(env.memory_list(Some("tabs")).is_empty());
}

fn text_blocks(messages: &Value) -> String {
    let mut out = String::new();
    for message in messages.as_array().into_iter().flatten() {
        for block in message["blocks"].as_array().into_iter().flatten() {
            if block["type"] == "text" {
                if let Some(text) = block["text"].as_str() {
                    out.push_str(text);
                    out.push('\n');
                }
            }
        }
    }
    out
}

#[test]
fn delegates_work_to_a_subagent_in_its_own_thread() {
    let env = TestEnv::new("subagent");
    let provider = env.with_scripted_provider();

    let result = env.run("delegate work to a subagent", &["--approval", "allow"]);
    assert_eq!(
        result["run"]["state"], "Succeeded",
        "lead run should succeed: {result}"
    );
    let session_id = result["sessionId"].as_str().expect("session id");

    let detail = env.show(session_id);
    let agents = detail["session"]["agents"]
        .as_array()
        .expect("agents array");
    assert_eq!(agents.len(), 2, "expected lead + subagent: {detail}");
    let lead = agents
        .iter()
        .find(|agent| agent["role"] == "lead")
        .expect("lead agent");
    let subagent = agents
        .iter()
        .find(|agent| agent["role"] == "subagent")
        .expect("subagent");
    assert_eq!(lead["status"], "succeeded");
    assert_eq!(subagent["status"], "succeeded");
    assert_eq!(subagent["model"], "small-model");
    assert_eq!(subagent["workMode"], "read_only");

    let main_text = text_blocks(&detail["session"]["messages"]);
    assert!(
        main_text.contains("Report from subagent"),
        "main thread should contain the report: {main_text}"
    );
    assert!(
        main_text.contains("Subagent report"),
        "report body should reach the lead: {main_text}"
    );
    assert!(
        !main_text.contains("Brief from the lead agent"),
        "the subagent trajectory leaked into the main thread: {main_text}"
    );

    let agent_id = subagent["id"].as_str().expect("agent id");
    let output = env.command(&["agents", "show", agent_id]);
    assert!(
        output.status.success(),
        "agents show failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let agent = last_json(&output.stdout);
    assert_eq!(agent["agent"]["id"], agent_id);
    let agent_text = text_blocks(&agent["messages"]);
    assert!(
        agent_text.contains("Brief from the lead agent"),
        "subagent thread should contain its brief: {agent_text}"
    );
    assert!(
        agent_text.contains("Subagent report"),
        "subagent thread should contain its report: {agent_text}"
    );

    let requests = provider.requests();
    let lead_request = requests
        .iter()
        .find(|request| {
            request["system"]
                .to_string()
                .contains("You are the lead agent")
        })
        .expect("lead request");
    let lead_tools: Vec<&str> = lead_request["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        lead_tools.contains(&"delegate"),
        "lead tools: {lead_tools:?}"
    );
    assert!(
        lead_tools.contains(&"wait_for_agents"),
        "lead tools: {lead_tools:?}"
    );

    let child_request = requests
        .iter()
        .find(|request| request["system"].to_string().contains("You are a subagent"))
        .expect("subagent request");
    let child_tools: Vec<&str> = child_request["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        !child_tools.contains(&"delegate"),
        "subagents must not see delegate: {child_tools:?}"
    );
    assert!(
        !child_tools.contains(&"wait_for_agents"),
        "subagents must not block-wait: {child_tools:?}"
    );
    assert!(
        !child_tools.contains(&"cancel_agent"),
        "subagents must not cancel peers: {child_tools:?}"
    );
    assert!(
        child_tools.contains(&"send_message"),
        "subagents can send directed messages: {child_tools:?}"
    );
    assert!(
        !child_tools.contains(&"fs_write") && !child_tools.contains(&"shell_exec"),
        "read_only subagents must not see write tools: {child_tools:?}"
    );
}

#[test]
fn delegation_rejects_a_model_more_capable_than_the_lead() {
    let env = TestEnv::new("subagent-scale");
    let _provider = env.with_scripted_provider();

    let result = env.run(
        "delegate to a model that is too capable",
        &["--approval", "allow", "--model", "small-model"],
    );
    assert_eq!(
        result["run"]["state"], "Succeeded",
        "run should recover from the rejected delegation: {result}"
    );
    let session_id = result["sessionId"].as_str().expect("session id");
    let detail = env.show(session_id);
    let agents = detail["session"]["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1, "no subagent should be created: {detail}");
    let tool_results = tool_result_text(&detail);
    assert!(
        tool_results.contains("more capable"),
        "the tool error should explain the scale cap: {tool_results}"
    );
}

const ONE_PIXEL_PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

struct ScriptedImageProvider {
    port: u16,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl ScriptedImageProvider {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind image provider");
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorder = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = handle_json_request(
                    stream,
                    recorder.clone(),
                    |_body| json!({ "data": [{ "b64_json": ONE_PIXEL_PNG_B64 }] }),
                );
            }
        });
        Self { port, requests }
    }
}

struct ScriptedDecisionProvider {
    port: u16,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl ScriptedDecisionProvider {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind decision provider");
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorder = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = handle_json_request(stream, recorder.clone(), |body| {
                    if body["questions"]["strategy"].is_object() {
                        json!({
                            "answers": {
                                "strategy": {
                                    "type": "choice",
                                    "choice": "head_tail",
                                    "probabilities": { "head_tail": 0.96, "keep_full": 0.04 },
                                    "confidence": 0.96
                                }
                            },
                            "usage": { "input_tokens": 8, "output_tokens": 0 }
                        })
                    } else {
                        json!({
                            "answers": {
                                "approve": { "type": "noul", "probability": 0.99 }
                            },
                            "usage": { "input_tokens": 8, "output_tokens": 0 }
                        })
                    }
                });
            }
        });
        Self { port, requests }
    }
}

fn handle_json_request(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<Value>>>,
    respond: impl Fn(&Value) -> Value,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if let Ok(mut log) = requests.lock() {
        log.push(request.clone());
    }
    let payload = serde_json::to_vec(&respond(&request)).unwrap();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        payload.len()
    )?;
    stream.write_all(&payload)?;
    stream.flush()
}

#[test]
fn image_generate_writes_a_local_asset() {
    let env = TestEnv::new("image");
    let provider = ScriptedImageProvider::start();
    let base_url = format!("http://127.0.0.1:{}", provider.port);
    let output = env.command(&[
        "image",
        "configure",
        "--provider",
        "openai_images",
        "--base-url",
        base_url.as_str(),
        "--model",
        "gpt-image-test",
        "--api-key",
        "literal:test-image-key",
        "--grant-egress",
    ]);
    assert!(
        output.status.success(),
        "image configure failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = last_json(&output.stdout);
    assert_eq!(status["ready"], true, "{status}");

    let out_path = env.workspace.join("generated.png");
    let output = env.command(&[
        "image",
        "generate",
        "--prompt",
        "a tiny cat",
        "--out",
        out_path.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "image generate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let generated = last_json(&output.stdout);
    assert_eq!(
        generated["assets"].as_array().unwrap().len(),
        1,
        "{generated}"
    );
    let bytes = std::fs::read(&out_path).expect("generated image");
    assert!(
        bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "output must be a PNG"
    );

    let output = env.command(&["image", "list"]);
    assert!(output.status.success());
    let listed = last_json(&output.stdout);
    assert_eq!(listed["assets"].as_array().unwrap().len(), 1, "{listed}");
    assert!(!provider.requests.lock().unwrap().is_empty());
}

#[test]
fn decision_triage_auto_approves_an_explicitly_enabled_shell() {
    let env = TestEnv::new("decision-triage");
    let _provider = env.with_scripted_provider();
    let decision = ScriptedDecisionProvider::start();
    let endpoint = format!("http://127.0.0.1:{}/decisions", decision.port);
    let output = env.command(&[
        "decision",
        "configure",
        "--endpoint",
        endpoint.as_str(),
        "--model",
        "typesafe/jev-test",
        "--api-key",
        "literal:decision-key",
        "--grant-egress",
        "--triage",
        "--triage-threshold",
        "0.9",
        "--triage-tool",
        "shell_exec",
    ]);
    assert!(
        output.status.success(),
        "decision configure failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = last_json(&output.stdout);
    assert_eq!(status["ready"], true, "{status}");

    let result = env.run("run an unsafe shell command", &["--approval", "deny"]);
    assert_eq!(
        result["run"]["state"], "Succeeded",
        "triage should let the run finish: {result}"
    );
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        results.contains("Windows") || results.contains("Linux") || results.contains("Darwin"),
        "triaged shell command output missing: {results}"
    );
    let requests = decision.requests.lock().unwrap();
    assert!(!requests.is_empty());
    let state = requests[0]["state"].as_str().unwrap_or_default();
    assert!(state.contains("shell_exec"), "{state}");
}

#[test]
fn decision_compaction_replaces_long_tool_output() {
    let env = TestEnv::new("compaction");
    let _provider = env.with_scripted_provider();
    let decision = ScriptedDecisionProvider::start();
    let endpoint = format!("http://127.0.0.1:{}/decisions", decision.port);
    let output = env.command(&[
        "decision",
        "configure",
        "--endpoint",
        endpoint.as_str(),
        "--model",
        "typesafe/jev-test",
        "--api-key",
        "literal:decision-key",
        "--grant-egress",
        "--compaction",
        "--compaction-threshold",
        "0.5",
        "--compaction-min-chars",
        "1000",
    ]);
    assert!(
        output.status.success(),
        "decision configure failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(env.workspace.join("LARGE.txt"), "x".repeat(10_000)).unwrap();

    let result = env.run("read the large file", &["--approval", "deny"]);
    assert_eq!(result["run"]["state"], "Succeeded", "{result}");
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        results.contains("truncated; full output"),
        "compaction marker missing: {results}"
    );
    assert!(
        results.chars().count() < 10_000,
        "compacted output should be shorter than the original"
    );
    let journal = std::fs::read_to_string(env.data_dir.join("journal").join("events.jsonl"))
        .expect("journal");
    assert!(
        journal.contains("ToolOutputCompacted"),
        "compaction must be journaled"
    );
}

#[test]
fn session_title_is_generated_and_applied() {
    let env = TestEnv::new("title");
    let _provider = env.with_scripted_provider();
    let first = env.run("hello", &["--title", "New session"]);
    let session_id = first["sessionId"].as_str().unwrap().to_string();

    let output = env.command(&[
        "roles",
        "set",
        "session_title",
        "--model",
        "test-model",
        "--grant-egress",
    ]);
    assert!(
        output.status.success(),
        "roles set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let second = env.run("inspect the workspace", &["--session", session_id.as_str()]);
    assert_eq!(second["run"]["state"], "Succeeded", "{second}");
    let list = env.command(&["sessions", "list"]);
    assert!(list.status.success());
    let listed = last_json(&list.stdout);
    let title = listed["sessions"][0]["title"].as_str().unwrap_or_default();
    assert_eq!(title, "Workspace inspection", "{listed}");
}

#[test]
fn memory_maintenance_merges_duplicates_and_keeps_dry_run_safe() {
    let env = TestEnv::new("maintenance");
    let _provider = env.with_scripted_provider();
    let output = env.command(&[
        "roles",
        "set",
        "memory_maintenance",
        "--model",
        "test-model",
        "--grant-egress",
    ]);
    assert!(
        output.status.success(),
        "roles set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    env.memory_add("The user prefers pnpm", "project");
    env.memory_add("The user uses pnpm for JavaScript", "project");
    let project = env.workspace.to_string_lossy().to_string();

    let dry = env.command(&[
        "memory",
        "maintain",
        "--project",
        project.as_str(),
        "--dry-run",
    ]);
    assert!(
        dry.status.success(),
        "dry run failed: {}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let report = last_json(&dry.stdout);
    assert_eq!(report["proposals"], 1, "{report}");
    assert_eq!(report["applied"], 0, "{report}");
    assert_eq!(report["pending"].as_array().unwrap().len(), 1, "{report}");

    let applied = env.command(&[
        "memory",
        "maintain",
        "--project",
        project.as_str(),
        "--apply",
    ]);
    assert!(
        applied.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let report = last_json(&applied.stdout);
    assert_eq!(report["applied"], 1, "{report}");

    let search = env.command(&["memory", "search", "Merged preference"]);
    assert!(search.status.success());
    let hits = last_json(&search.stdout);
    assert!(
        !hits["hits"].as_array().unwrap_or(&Vec::new()).is_empty(),
        "the merged fact should be searchable: {hits}"
    );
}

#[test]
fn the_agent_can_invoke_image_generate() {
    let env = TestEnv::new("image-tool");
    let provider = env.with_scripted_provider();
    let image = ScriptedImageProvider::start();
    let base_url = format!("http://127.0.0.1:{}", image.port);
    let output = env.command(&[
        "image",
        "configure",
        "--provider",
        "openai_images",
        "--base-url",
        base_url.as_str(),
        "--model",
        "gpt-image-test",
        "--api-key",
        "literal:test-image-key",
        "--grant-egress",
    ]);
    assert!(
        output.status.success(),
        "image configure failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result = env.run("draw an image", &["--approval", "deny"]);
    assert_eq!(result["run"]["state"], "Succeeded", "{result}");
    let session = env.show(result["sessionId"].as_str().unwrap());
    let results = tool_result_text(&session);
    assert!(
        results.contains("assets"),
        "the image tool result should carry asset ids: {results}"
    );
    let journal = std::fs::read_to_string(env.data_dir.join("journal").join("events.jsonl"))
        .expect("journal");
    assert!(
        journal.contains("ImageGenerated"),
        "the image must be journaled"
    );

    let image_requests = image.requests.lock().unwrap();
    assert_eq!(image_requests.len(), 1);
    assert_eq!(image_requests[0]["prompt"], "a red cube on a table");

    let requests = provider.requests();
    let lead_tools: Vec<&str> = requests[0]["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        lead_tools.contains(&"image_generate"),
        "the configured image tool must be exposed: {lead_tools:?}"
    );
}
