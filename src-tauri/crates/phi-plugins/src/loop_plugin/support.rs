use phi_domain::agent::AgentStatus;
use phi_domain::events::DomainEvent;
use phi_domain::view::RunState;

use super::RunContext;

pub(super) fn set_status(ctx: &RunContext, state: RunState) {
    let mut status = ctx.status.lock().unwrap();
    status.state = state;
}

pub(super) fn publish_agent_status(ctx: &RunContext, status: AgentStatus, note: Option<String>) {
    let _ = ctx.bus.publish(
        &ctx.session_id,
        "plugin-loop",
        &DomainEvent::AgentStatusChanged {
            agent_id: ctx.agent_id.clone(),
            session_id: ctx.session_id.clone(),
            status,
            note,
        },
    );
}

pub(super) fn set_status_progress(ctx: &RunContext, steps: u64) {
    let mut status = ctx.status.lock().unwrap();
    status.steps = steps;
    if status.state == RunState::Scheduled {
        status.state = RunState::Running;
    }
}

pub(super) fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub(super) fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !(value == "0" || value == "false" || value == "off" || value == "no")
        })
        .unwrap_or(default)
}
