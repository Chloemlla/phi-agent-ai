//! `run` command parsing.

use phi_i18n as i18n;

use super::common::take_flag;
use super::{ApprovalMode, RunArgs};

pub(super) fn parse_run(rest: &[String]) -> Result<RunArgs, String> {
    let mut args = RunArgs {
        message: None,
        session_id: None,
        provider: None,
        model: None,
        thinking: None,
        approval: ApprovalMode::Deny,
        stream: true,
        title: None,
    };
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--message")? {
            args.message = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--session")? {
            args.session_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--provider")? {
            args.provider = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--model")? {
            args.model = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--thinking")? {
            args.thinking = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--title")? {
            args.title = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--approval")? {
            args.approval = match value.trim().to_ascii_lowercase().as_str() {
                "deny" => ApprovalMode::Deny,
                "allow" | "auto" => ApprovalMode::Allow,
                "prompt" | "ask" => ApprovalMode::Prompt,
                other => {
                    return Err(i18n::t_args(
                        "Unknown approval mode {mode}; expected deny, allow or prompt",
                        &[("mode", other.to_string())],
                    ));
                }
            };
        } else {
            match rest[index].as_str() {
                "--no-stream" => {
                    args.stream = false;
                    index += 1;
                }
                "--stream" => {
                    args.stream = true;
                    index += 1;
                }
                other => {
                    return Err(i18n::t_args(
                        "Unknown run argument: {arg}",
                        &[("arg", other.to_string())],
                    ));
                }
            }
        }
    }
    Ok(args)
}
