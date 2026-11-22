//! `image` command parsing.

use std::path::PathBuf;

use phi_contracts::auxiliary::ImageConfigInput;
use phi_i18n as i18n;

use super::Command;
use super::common::{parse_id, take_flag};

pub(super) fn parse_image(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Err(i18n::t(
            "image requires a subcommand: configure, show, generate, list or delete",
        ));
    };
    let args = &rest[1..];
    match sub {
        "configure" => {
            let mut input = ImageConfigInput {
                enabled: true,
                format: "png".to_string(),
                ..Default::default()
            };
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--provider")? {
                    input.provider = value;
                } else if let Some(value) = take_flag(args, &mut index, "--base-url")? {
                    input.base_url = value;
                } else if let Some(value) = take_flag(args, &mut index, "--model")? {
                    input.model = value;
                } else if let Some(value) = take_flag(args, &mut index, "--api-key")? {
                    input.api_key = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--size")? {
                    input.size = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--quality")? {
                    input.quality = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--format")? {
                    input.format = value;
                } else if let Some(value) = take_flag(args, &mut index, "--max-per-turn")? {
                    input.max_per_turn = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--max-per-turn".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--max-bytes")? {
                    input.max_bytes = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--max-bytes".to_string()), ("value", value)],
                        )
                    })?;
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
            Ok(Command::ImageConfigure(input))
        }
        "show" => Ok(Command::ImageShow),
        "generate" => {
            let mut prompt = None;
            let mut session_id = None;
            let mut size = None;
            let mut count = 1;
            let mut out = None;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--prompt")? {
                    prompt = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--session")? {
                    session_id = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--size")? {
                    size = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--count")? {
                    count = value.parse().map_err(|_| {
                        i18n::t_args(
                            "Invalid number for {flag}: {value}",
                            &[("flag", "--count".to_string()), ("value", value)],
                        )
                    })?;
                } else if let Some(value) = take_flag(args, &mut index, "--out")? {
                    out = Some(PathBuf::from(value));
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::ImageGenerate {
                prompt: prompt.ok_or_else(|| i18n::t("--prompt is required"))?,
                session_id,
                size,
                count,
                out,
            })
        }
        "list" => {
            let mut session_id = None;
            let mut index = 0;
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--session")? {
                    session_id = Some(value);
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::ImageList { session_id })
        }
        "delete" => Ok(Command::ImageDelete(parse_id(args, 0)?)),
        other => Err(i18n::t_args(
            "Unknown image subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}
