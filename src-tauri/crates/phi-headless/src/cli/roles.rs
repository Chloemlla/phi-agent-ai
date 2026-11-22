//! `roles` command parsing.

use phi_contracts::auxiliary::AuxiliaryRoleInput;
use phi_i18n as i18n;

use super::Command;
use super::common::{parse_id, take_flag};

pub(super) fn parse_roles(rest: &[String]) -> Result<Command, String> {
    let Some(sub) = rest.first().map(String::as_str) else {
        return Err(i18n::t("roles requires a subcommand: list, set or clear"));
    };
    let args = &rest[1..];
    match sub {
        "list" => Ok(Command::RolesList),
        "set" => {
            let role = args
                .first()
                .filter(|value| !value.starts_with('-'))
                .cloned()
                .ok_or_else(|| i18n::t("roles set requires a role name"))?;
            let mut input = AuxiliaryRoleInput::default();
            let mut index = usize::from(!args.is_empty());
            while index < args.len() {
                if let Some(value) = take_flag(args, &mut index, "--provider")? {
                    input.provider_id = Some(value);
                } else if let Some(value) = take_flag(args, &mut index, "--model")? {
                    input.model = value;
                } else if let Some(value) = take_flag(args, &mut index, "--thinking")? {
                    input.thinking = Some(value);
                } else if args[index] == "--grant-egress" {
                    input.egress_granted = true;
                    index += 1;
                } else {
                    return Err(i18n::t_args(
                        "Unknown argument: {arg}",
                        &[("arg", args[index].clone())],
                    ));
                }
            }
            Ok(Command::RolesSet { role, input })
        }
        "clear" => Ok(Command::RolesClear(parse_id(args, 0)?)),
        other => Err(i18n::t_args(
            "Unknown roles subcommand: {command}",
            &[("command", other.to_string())],
        )),
    }
}
