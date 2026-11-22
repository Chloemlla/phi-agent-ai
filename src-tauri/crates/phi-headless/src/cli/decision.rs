//! `decision` command parsing.

use phi_contracts::auxiliary::DecisionConfigInput;
use phi_i18n as i18n;

use super::Command;
use super::common::take_flag;

pub(super) fn parse_decision(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Err(i18n::t(
            "decision requires a subcommand: configure, show or test",
        ));
    };
    let args = &rest[1..];
    match sub {
        "configure" => {
            let mut input = DecisionConfigInput {
                enabled: true,
                timeout_ms: 8_000,
                triage_threshold: 0.92,
                triage_tools: vec!["fs_write".to_string()],
                compaction_threshold: 0.8,
                compaction_min_chars: 8_000,
                ..Default::default()
            };
            let mut custom_tools = false;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--endpoint")? {
                    input.endpoint = value;
                } else if let Some(value) = take_flag(args, &mut index, "--model")? {
                    input.model = value;
                } else if let Some(value) = take_flag(args, &mut index, "--api-key")? {
                    input.api_key = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--timeout-ms")? {
                    input.timeout_ms = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--timeout-ms".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--triage-threshold")? {
                    input.triage_threshold = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--triage-threshold".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--triage-tool")? {
                    if !custom_tools {
                        input.triage_tools.clear();
                        custom_tools = true;
                    }
                    input.triage_tools.push(value);
                } else if let Some(value) = take_flag(args, &mut index, "--compaction-threshold")? {
                    input.compaction_threshold = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[
                                ("flag", "--compaction-threshold".to_string()),
                                ("value", value),
                            ],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--compaction-min-chars")? {
                    input.compaction_min_chars = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[
                                ("flag", "--compaction-min-chars".to_string()),
                                ("value", value),
                            ],
                        )
                    })?;
                } else if args[index] == "--triage" {
                    input.triage_enabled = true;
                    index += 1;
                } else if args[index] == "--compaction" {
                    input.compaction_enabled = true;
                    index += 1;
                } else if args[index] == "--grant-egress" {
                    input.egress_granted = true;
                    index += 1;
                } else if args[index] == "--disabled" {
                    input.enabled = false;
                    index += 1;
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::DecisionConfigure(input))
        }
        "show" => Ok(Command::DecisionShow),
        "test" => {
            let mut state = None;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--state")? {
                    state = Some(value);
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::DecisionTest {
                state: state.ok_or_else(|| i18n::t("--state is required"))?,
            })
        }
        other => Err(i18n::t_args(
            "Unknown decision subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}
