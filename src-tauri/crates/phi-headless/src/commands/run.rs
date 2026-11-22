//! `run` command implementation, including approval handling and streaming.

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use phi_adapters::jsonl_sink::{JsonlEventSink, JsonlStreamSink};
use phi_contracts::stream::StreamPort;
use phi_domain::view::{RunState, RunView};
use phi_i18n as i18n;
use phi_kernel::event_bus::EventSink;
use phi_node::{Node, NodeConfig, compose};

use crate::cli::{ApprovalMode, GlobalOptions, RunArgs};

use super::support::print_json;

pub(super) async fn run(global: GlobalOptions, args: RunArgs) -> Result<i32, String> {
    let event_sink: Arc<dyn EventSink> = if global.quiet {
        Arc::new(JsonlEventSink::quiet())
    } else {
        Arc::new(JsonlEventSink::new())
    };
    let stream_sink: Option<Arc<dyn StreamPort>> = if args.stream {
        Some(Arc::new(JsonlStreamSink))
    } else {
        None
    };
    let node = compose(NodeConfig {
        data_dir: global.data_dir.clone(),
        workspace: global.workspace.clone(),
        fallback_workspace: None,
        event_sinks: vec![event_sink],
        stream_sink,
        language: global.language.clone(),
        writable: true,
    })?;

    if let Some(provider) = args.provider.as_deref() {
        node.set_active_provider(provider)?;
    }

    let message = match args.message {
        Some(message) if message.trim() == "-" => read_stdin()?,
        Some(message) => message,
        None => read_stdin()?,
    };
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err(i18n::t(
            "Message is empty (use --message or pipe text through stdin)",
        ));
    }

    let session_id = match args.session_id {
        Some(id) => {
            node.get_session(&id)?;
            id
        }
        None => {
            let title = args
                .title
                .unwrap_or_else(|| message.chars().take(24).collect::<String>());
            node.create_session(Some(&title))?.id
        }
    };

    let worker = tokio::spawn(approval_worker(
        node.clone(),
        session_id.clone(),
        args.approval,
    ));
    let run_id = node.send_message_full(
        &session_id,
        &message,
        None,
        args.model.as_deref(),
        args.thinking.as_deref(),
        &[],
    );
    let run_id = match run_id {
        Ok(run_id) => run_id,
        Err(error) => {
            worker.abort();
            return Err(error);
        }
    };

    let result = wait_for_terminal(&node, &session_id, &run_id).await;
    worker.abort();
    let run = result?;
    let auxiliary = node.settle_auxiliary(&session_id).await;

    print_json(&serde_json::json!({
        "type": "result",
        "sessionId": session_id,
        "runId": run_id,
        "run": run,
        "auxiliary": auxiliary,
    }))?;
    eprintln!(
        "[phi-headless] run {} {} steps={} tokens_in={} tokens_out={}{}",
        run.id,
        run.state,
        run.steps,
        run.input_tokens,
        run.output_tokens,
        run.error
            .as_deref()
            .map(|error| format!(" ({error})"))
            .unwrap_or_default()
    );

    Ok(match run.state {
        RunState::Succeeded => 0,
        RunState::Aborted => 3,
        RunState::AwaitingReconciliation => 5,
        _ => 4,
    })
}

async fn wait_for_terminal(node: &Node, session_id: &str, run_id: &str) -> Result<RunView, String> {
    loop {
        if let Some(session) = node.session_view(session_id) {
            if let Some(run) = session.runs.iter().find(|run| run.id == run_id) {
                if run.state.is_terminal() {
                    return Ok(run.clone());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn approval_worker(node: Arc<Node>, session_id: String, mode: ApprovalMode) {
    loop {
        for approval in node.pending_approvals(&session_id) {
            let (approved, note) = match mode {
                ApprovalMode::Deny => (false, "headless: denied by --approval deny".to_string()),
                ApprovalMode::Allow => (
                    true,
                    "headless: auto-approved by --approval allow (tests only)".to_string(),
                ),
                ApprovalMode::Prompt => {
                    eprintln!(
                        "[approval] {} tool={} risk={}",
                        approval.id, approval.tool_id, approval.risk
                    );
                    eprintln!("[approval] args={}", approval.args_preview);
                    eprintln!("[approval] approve? [y/N]");
                    let line = tokio::task::spawn_blocking(prompt_line)
                        .await
                        .unwrap_or_default();
                    let approved = matches!(
                        line.trim().to_ascii_lowercase().as_str(),
                        "y" | "yes" | "approve"
                    );
                    (
                        approved,
                        format!(
                            "headless: interactive ({})",
                            if approved { "approved" } else { "rejected" }
                        ),
                    )
                }
            };
            if let Err(error) = node.approve(&approval.id, approved, Some(note)) {
                eprintln!(
                    "[phi-headless] {}",
                    i18n::t_args("Approval failed: {error}", &[("error", error)])
                );
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn prompt_line() -> String {
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    line
}

fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| {
            i18n::t_args(
                "Failed to read stdin: {error}",
                &[("error", error.to_string())],
            )
        })?;
    Ok(text)
}
