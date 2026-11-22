//! `memory` command parsing.

use phi_contracts::memory::{
    ForgetSelector, MemoryAddRequest, MemoryEpisodeInput, MemoryListFilter, MemoryQuery,
};
use phi_domain::memory::{EpisodeSource, MemoryScope, MemoryState, SourceTrust};
use phi_i18n as i18n;

use super::Command;
use super::common::{parse_id, take_flag};

pub(super) fn parse_memory(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Ok(Command::MemoryList(MemoryListFilter::default()));
    };
    // `memory --text foo` is accepted as `memory list --text foo`.
    if sub.starts_with('-') {
        return Ok(Command::MemoryList(parse_memory_list(rest)?));
    }
    let args = &rest[1..];
    match sub {
        "list" | "ls" => Ok(Command::MemoryList(parse_memory_list(args)?)),
        "search" | "find" => Ok(Command::MemorySearch(parse_memory_search(args)?)),
        "show" => Ok(Command::MemoryShow(parse_id(args, 0)?)),
        "add" | "remember" => Ok(Command::MemoryAdd(parse_memory_add(args)?)),
        "invalidate" => parse_memory_invalidate(args),
        "correct" | "fix" => parse_memory_correct(args),
        "supersede" | "replace" => parse_memory_supersede(args),
        "forget" | "delete" => parse_memory_forget(args),
        "maintain" | "maintenance" => parse_memory_maintain(args),
        "stats" => Ok(Command::MemoryStats),
        other => Err(i18n::t_args(
            "Unknown memory subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}

fn parse_memory_maintain(args: &[String]) -> Result<Command, String> {
    let mut dry_run = true;
    let mut project_id = None;
    let mut index = 0;
    while index < args.len() {
        if let Some(value) = take_flag(args, &mut index, "--project")? {
            project_id = Some(value);
        } else if args[index] == "--apply" {
            dry_run = false;
            index += 1;
        } else if args[index] == "--dry-run" {
            dry_run = true;
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown argument: {arg}",
                &[("arg", args[index].clone())],
            ));
        }
    }
    Ok(Command::MemoryMaintain {
        project_id,
        dry_run,
    })
}

fn parse_memory_scope(raw: &str) -> Result<MemoryScope, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(MemoryScope::User),
        "project" => Ok(MemoryScope::Project),
        "agent" => Ok(MemoryScope::Agent),
        "run" => Ok(MemoryScope::Run),
        other => Err(i18n::t_args(
            "Unknown memory scope {scope}; expected user, project, agent or run",
            &[("scope", other.to_string())],
        )),
    }
}

fn parse_memory_trust(raw: &str) -> Result<SourceTrust, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(SourceTrust::User),
        "system" => Ok(SourceTrust::System),
        "tool" => Ok(SourceTrust::Tool),
        "model" => Ok(SourceTrust::Model),
        "external" => Ok(SourceTrust::External),
        other => Err(i18n::t_args(
            "Unknown memory source {source}; expected user, system, tool, model or external",
            &[("source", other.to_string())],
        )),
    }
}

fn parse_memory_state(raw: &str) -> Result<MemoryState, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "active" => Ok(MemoryState::Active),
        "invalidated" => Ok(MemoryState::Invalidated),
        "superseded" => Ok(MemoryState::Superseded),
        other => Err(i18n::t_args(
            "Unknown memory state {state}; expected active, invalidated or superseded",
            &[("state", other.to_string())],
        )),
    }
}

fn parse_episode_source(raw: &str) -> Result<EpisodeSource, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user_input" | "user" => Ok(EpisodeSource::UserInput),
        "tool_result" | "tool" => Ok(EpisodeSource::ToolResult),
        "model_output" | "model" => Ok(EpisodeSource::ModelOutput),
        "external" => Ok(EpisodeSource::External),
        other => Err(i18n::t_args(
            "Unknown episode source {source}; expected user_input, tool_result, model_output or external",
            &[("source", other.to_string())],
        )),
    }
}

fn parse_memory_list(rest: &[String]) -> Result<MemoryListFilter, String> {
    let mut filter = MemoryListFilter::default();
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--text")? {
            filter.text = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            filter.scopes.push(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--state")? {
            filter.states.push(parse_memory_state(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--project")? {
            filter.project_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--run")? {
            filter.run_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--limit")? {
            filter.limit = value.parse().map_err(|_| {
                i18n::t_args(
                    "--limit is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?;
        } else if let Some(value) = take_flag(rest, &mut index, "--offset")? {
            filter.offset = value.parse().map_err(|_| {
                i18n::t_args(
                    "--offset is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?;
        } else if !rest[index].starts_with('-') {
            filter.text = Some(rest[index].clone());
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory list argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    Ok(filter)
}

fn parse_memory_search(rest: &[String]) -> Result<MemoryQuery, String> {
    let mut query = MemoryQuery {
        limit: 10,
        ..Default::default()
    };
    let mut parts: Vec<String> = Vec::new();
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            query.scopes.push(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--limit")? {
            query.limit = value.parse().map_err(|_| {
                i18n::t_args(
                    "--limit is not a number: {value}",
                    &[("value", value.clone())],
                )
            })?;
        } else if let Some(value) = take_flag(rest, &mut index, "--min-score")? {
            query.min_score = parse_finite_f64("--min-score", &value)?;
        } else if rest[index] == "--include-inactive" {
            query.include_inactive = true;
            index += 1;
        } else if !rest[index].starts_with('-') {
            parts.push(rest[index].clone());
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory search argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    if parts.is_empty() {
        return Err(i18n::t("memory search requires a query"));
    }
    query.text = parts.join(" ");
    Ok(query)
}

fn parse_memory_add(rest: &[String]) -> Result<MemoryAddRequest, String> {
    let mut fact = None;
    let mut scope = MemoryScope::User;
    let mut trust = SourceTrust::User;
    let mut confidence = None;
    let mut valid_from = None;
    let mut valid_to = None;
    let mut episode = None;
    let mut episode_source = EpisodeSource::UserInput;
    let mut episode_label = None;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--fact")? {
            fact = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            scope = parse_memory_scope(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--source")? {
            trust = parse_memory_trust(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--confidence")? {
            confidence = Some(parse_finite_f64("--confidence", &value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-from")? {
            valid_from = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-to")? {
            valid_to = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--episode")? {
            episode = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--episode-source")? {
            episode_source = parse_episode_source(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--episode-label")? {
            episode_label = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else {
            return Err(i18n::t_args(
                "Unknown memory add argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    let fact = fact.ok_or_else(|| i18n::t("memory add requires --fact TEXT"))?;
    if scope == MemoryScope::Run {
        return Err(i18n::t(
            "Run-scoped memories are written by the agent inside a run",
        ));
    }
    Ok(MemoryAddRequest {
        fact,
        scope,
        source_trust: trust,
        confidence,
        valid_from,
        valid_to,
        episode: episode.map(|content| MemoryEpisodeInput {
            content,
            source: episode_source,
            source_label: episode_label,
            session_id: None,
        }),
        reason,
    })
}

fn require_reason(value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| i18n::t("This memory command requires --reason TEXT"))
}

fn parse_finite_f64(flag: &str, value: &str) -> Result<f64, String> {
    let parsed: f64 = value.parse().map_err(|_| {
        i18n::t_args(
            "{flag} is not a number: {value}",
            &[("flag", flag.to_string()), ("value", value.to_string())],
        )
    })?;
    if !parsed.is_finite() {
        return Err(i18n::t_args(
            "{flag} must be a finite number: {value}",
            &[("flag", flag.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(parsed)
}

fn parse_memory_invalidate(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--id")? {
            if value != id {
                return Err(i18n::t("Conflicting memory ids"));
            }
        } else if rest[index] == id {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory invalidate argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    Ok(Command::MemoryInvalidate {
        id,
        reason: require_reason(reason)?,
    })
}

fn parse_memory_correct(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut fact = None;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--fact")? {
            fact = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--id")? {
            if value != id {
                return Err(i18n::t("Conflicting memory ids"));
            }
        } else if rest[index] == id {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory correct argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    Ok(Command::MemoryCorrect {
        id,
        fact: fact.ok_or_else(|| i18n::t("memory correct requires --fact TEXT"))?,
        reason: require_reason(reason)?,
    })
}

fn parse_memory_supersede(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut fact = None;
    let mut scope = None;
    let mut trust = SourceTrust::User;
    let mut confidence = None;
    let mut valid_from = None;
    let mut valid_to = None;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--fact")? {
            fact = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            scope = Some(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--source")? {
            trust = parse_memory_trust(&value)?;
        } else if let Some(value) = take_flag(rest, &mut index, "--confidence")? {
            confidence = Some(parse_finite_f64("--confidence", &value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-from")? {
            valid_from = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--valid-to")? {
            valid_to = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--id")? {
            if value != id {
                return Err(i18n::t("Conflicting memory ids"));
            }
        } else if rest[index] == id {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory supersede argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    let fact = fact.ok_or_else(|| i18n::t("memory supersede requires --fact TEXT"))?;
    let scope = scope.ok_or_else(|| i18n::t("memory supersede requires --scope"))?;
    if scope == MemoryScope::Run {
        return Err(i18n::t(
            "Run-scoped memories are written by the agent inside a run",
        ));
    }
    Ok(Command::MemorySupersede {
        id,
        request: MemoryAddRequest {
            fact,
            scope,
            source_trust: trust,
            confidence,
            valid_from,
            valid_to,
            episode: None,
            reason: None,
        },
        reason: require_reason(reason)?,
    })
}

fn parse_memory_forget(rest: &[String]) -> Result<Command, String> {
    let mut selector = ForgetSelector::default();
    let mut all = false;
    let mut reason = None;
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--id")? {
            selector.ids.push(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--scope")? {
            selector.scopes.push(parse_memory_scope(&value)?);
        } else if let Some(value) = take_flag(rest, &mut index, "--project")? {
            selector.project_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--run")? {
            selector.run_id = Some(value);
        } else if let Some(value) = take_flag(rest, &mut index, "--reason")? {
            reason = Some(value);
        } else if rest[index] == "--all" {
            all = true;
            index += 1;
        } else if !rest[index].starts_with('-') {
            selector.ids.push(rest[index].clone());
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown memory forget argument: {arg}",
                &[("arg", rest[index].clone())],
            ));
        }
    }
    let has_selector = selector.has_selector();
    if all && has_selector {
        return Err(i18n::t("Do not combine --all with other forget selectors"));
    }
    if !all && !has_selector {
        return Err(i18n::t(
            "memory forget requires --id, --scope, --project, --run or --all",
        ));
    }
    selector.all = all;
    Ok(Command::MemoryForget { selector, reason })
}
