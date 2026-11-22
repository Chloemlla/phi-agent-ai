//! `providers` command parsing.

use phi_contracts::provider::{ModelSpec, ProviderInput};
use phi_i18n as i18n;

use super::Command;
use super::common::{parse_id, take_flag};

pub(super) fn parse_providers(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Ok(Command::ProvidersList);
    };
    let args = &rest[1..];
    match sub {
        "list" | "ls" => Ok(Command::ProvidersList),
        "show" => Ok(Command::ProvidersShow(parse_id(args, 0)?)),
        "add" => {
            let (input, activate) = parse_provider_input(args, false)?;
            Ok(Command::ProvidersAdd { input, activate })
        }
        "update" | "edit" => {
            let (input, activate) = parse_provider_input(args, true)?;
            Ok(Command::ProvidersUpdate { input, activate })
        }
        "remove" | "rm" => Ok(Command::ProvidersRemove(parse_id(args, 0)?)),
        "use" | "activate" => Ok(Command::ProvidersUse(parse_id(args, 0)?)),
        "test" => Ok(Command::ProvidersTest(optional_id(args)?)),
        "models" => Ok(Command::ProvidersModels(optional_id(args)?)),
        other => Err(i18n::t_args(
            "Unknown providers subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

fn optional_id(rest: &[String]) -> Result<Option<String>, String> {
    match parse_id(rest, 0) {
        Ok(id) => Ok(Some(id)),
        Err(_) => Ok(None),
    }
}

fn parse_provider_input(
    rest: &[String],
    require_id: bool,
) -> Result<(ProviderInput, bool), String> {
    let mut input = ProviderInput::default();
    let mut activate = false;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--id")? {
            input.id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--name")? {
            input.name = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--protocol")? {
            input.protocol = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--base-url")? {
            input.base_url = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--api-key")? {
            input.api_key = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--model")? {
            input.model = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--auth")? {
            input.auth = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--models")? {
            input.models = Some(
                value
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .map(ModelSpec::from_id)
                    .collect(),
            );
        } else if let Some(value) = take_flag(rest, &mut index, "--model-map")? {
            let mut map = std::collections::HashMap::new();
            for pair in value.split(',') {
                let Some((key, value)) = pair.split_once('=') else {
                    return Err(i18n::t_args(
                        "--model-map expects key=model: {pair}",
                        &[("pair", pair.to_string())],
                    ));
                };
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
            input.model_map = Some(map);
        } else if let Some(value) = take_flag(rest, &mut index, "--max-output-tokens")? {
            input.max_output_tokens = Some(value.parse().map_err(|_| {
                i18n::t_args(
                    "--max-output-tokens is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?);
        } else if let Some(value) = take_flag(rest, &mut index, "--temperature")? {
            input.temperature = Some(value.parse().map_err(|_| {
                i18n::t_args(
                    "--temperature is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?);
        } else if let Some(value) = take_flag(rest, &mut index, "--sort-index")? {
            input.sort_index = Some(value.parse().map_err(|_| {
                i18n::t_args(
                    "--sort-index is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?);
        } else if let Some(value) = take_flag(rest, &mut index, "--icon")? {
            input.icon = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--icon-color")? {
            input.icon_color = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--website")? {
            input.website_url = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--notes")? {
            input.notes = Some(value);
        } else {
            match rest[index].as_str() {
                "--activate" => {
                    activate = true;
                    index += 1;
                }
                "--disabled" => {
                    input.enabled = Some(false);
                    index += 1;
                }
                value if !value.starts_with('-') => {
                    positional.push(value.to_string());
                    index += 1;
                }
                other => {
                    return Err(i18n::t_args(
                        "Unknown provider argument: {arg}",
                        &[("arg", other.to_string())],
                    ));
                }
            }
        }
    }
    if input.id.is_none() {
        if let Some(first) = positional.first() {
            input.id = Some(first.clone());
        }
    }
    if require_id && input.id.is_none() {
        return Err(i18n::t("update requires a provider id"));
    }
    Ok((input, activate))
}
