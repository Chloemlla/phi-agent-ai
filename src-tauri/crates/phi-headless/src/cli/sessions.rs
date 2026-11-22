//! `sessions` and `agents` command parsing.

use phi_i18n as i18n;

use super::Command;
use super::common::parse_id;

pub(super) fn parse_sessions(rest: &[String]) -> Result<Command, String> {
    match rest.first().map(String::as_str) {
        Some("list") | Some("ls") | None => Ok(Command::SessionsList),
        Some("show") => Ok(Command::SessionsShow(parse_id(rest, 1)?)),
        Some("delete") | Some("rm") => Ok(Command::SessionsDelete(parse_id(rest, 1)?)),
        Some(other) => Err(i18n::t_args(
            "Unknown sessions subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

pub(super) fn parse_agents(rest: &[String]) -> Result<Command, String> {
    match rest.first().map(String::as_str) {
        Some("list") => Ok(Command::AgentsList(parse_id(rest, 1)?)),
        Some("show") => Ok(Command::AgentsShow(parse_id(rest, 1)?)),
        Some(other) => Err(i18n::t_args(
            "Unknown agents subcommand: {command}",
            &[("command", other.to_string())],
        )),
        None => Err(i18n::t(
            "agents requires a subcommand: list <session-id> or show <agent-id>",
        )),
    }
}
