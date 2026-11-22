//! `approve` command parsing.

use phi_i18n as i18n;

use super::Command;
use super::common::{parse_id, take_flag};

pub(super) fn parse_approve(rest: &[String]) -> Result<Command, String> {
    let id = parse_id(rest, 0)?;
    let mut approved = None;
    let mut note = None;
    let mut index = 0;
    while index < rest.len() {
        let arg = &rest[index];
        if arg == "--allow" || arg == "--approve" {
            approved = Some(true);
            index += 1;
        } else if arg == "--deny" || arg == "--reject" {
            approved = Some(false);
            index += 1;
        } else if let Some(value) = take_flag(rest, &mut index, "--note")? {
            note = Some(value);
        } else if arg.as_str() == id.as_str() {
            index += 1;
        } else {
            return Err(i18n::t_args(
                "Unknown approve argument: {arg}",
                &[("arg", arg.clone())],
            ));
        }
    }
    let approved = approved.ok_or_else(|| i18n::t("approve requires --allow or --deny"))?;
    Ok(Command::Approve { id, approved, note })
}
