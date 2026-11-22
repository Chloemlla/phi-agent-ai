//! Shared helpers used by the individual command parsers.

use phi_i18n as i18n;

pub(super) fn parse_id(rest: &[String], position: usize) -> Result<String, String> {
    if let Some(value) = rest.get(position) {
        if !value.starts_with('-') {
            return Ok(value.clone());
        }
    }
    let mut index = 0;
    while index < rest.len() {
        if let Some(value) = take_flag(rest, &mut index, "--id")? {
            return Ok(value);
        }
        index += 1;
    }
    Err(i18n::t("Missing id"))
}

pub(super) fn take_flag(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<Option<String>, String> {
    let arg = args.get(*index).ok_or_else(|| {
        i18n::t_args(
            "Internal error: index out of bounds ({flag})",
            &[("flag", flag.to_string())],
        )
    })?;
    if arg == flag {
        let value = args.get(*index + 1).cloned().ok_or_else(|| {
            i18n::t_args("{flag} requires a value", &[("flag", flag.to_string())])
        })?;
        *index += 2;
        return Ok(Some(value));
    }
    let prefix = format!("{flag}=");
    if let Some(value) = arg.strip_prefix(&prefix) {
        *index += 1;
        return Ok(Some(value.to_string()));
    }
    Ok(None)
}
