use std::sync::Arc;

use phi_contracts::SLOT_POLICY_EVALUATE;
use phi_contracts::ports::{PolicyDecision, PolicyEffect, PolicyInput, PolicyPort, SandboxPort};
use phi_i18n as i18n;
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::registry::Contribution;

/// Default policy for the MVP.
///
/// - reads inside the workspace: allow
/// - writes inside the workspace: allow (previous content is backed up)
/// - shell execution: read-only and project-verification commands are allowed
///   without approval; anything else asks, with an explicit deny list for
///   destructive commands
/// - anything else / outside the workspace: deny
/// - no matching rule = deny (least privilege)
pub struct DefaultPolicyPlugin {
    sandbox: Arc<dyn SandboxPort>,
}

impl DefaultPolicyPlugin {
    pub fn new(sandbox: Arc<dyn SandboxPort>) -> Self {
        Self { sandbox }
    }

    fn inside(&self, input: &PolicyInput) -> Result<(), String> {
        let requested = input
            .args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        self.sandbox.resolve_path(&input.workspace, requested)?;
        Ok(())
    }
}

impl Plugin for DefaultPolicyPlugin {
    fn id(&self) -> &'static str {
        "plugin-policy"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_POLICY_EVALUATE.to_string(),
            id: "default".to_string(),
        }])
    }
}

const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf ~",
    "format c:",
    "format d:",
    "del /f /s /q c:\\",
    "rd /s /q c:\\",
    "shutdown",
    "mkfs",
    "dd if=",
    ":(){",
    "> /dev/sda",
];

/// Classification for the safe-command whitelist. This is a security gate,
/// unlike the presentation-only classifier in the frontend: it is deliberately
/// conservative and returns `Other` for anything it cannot parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellClass {
    ReadOnly,
    Verify,
    Other,
}

const VERIFY_SCRIPTS: &[&str] = &[
    "test",
    "tests",
    "build",
    "lint",
    "typecheck",
    "type-check",
    "check",
    "fmt",
    "format",
];

fn classify_shell(command: &str) -> ShellClass {
    let normalized = strip_safe_redirections(command);
    if normalized.trim().is_empty()
        || normalized.contains('`')
        || normalized.contains("$(")
        || normalized.contains("${")
        || normalized
            .chars()
            .any(|character| matches!(character, '<' | '>' | '^' | '%' | '!'))
        || has_single_ampersand(&normalized)
    {
        return ShellClass::Other;
    }

    let segments = split_segments(&normalized);
    if segments.is_empty() {
        return ShellClass::Other;
    }

    let mut class = ShellClass::ReadOnly;
    let mut meaningful = false;
    for segment in segments {
        let Some(words) = shell_words(&segment) else {
            return ShellClass::Other;
        };
        let Some((command, args)) = words.split_first() else {
            continue;
        };
        match classify_segment(command, args) {
            Some(ShellClass::ReadOnly) => {
                meaningful = true;
            }
            Some(ShellClass::Verify) => {
                meaningful = true;
                class = ShellClass::Verify;
            }
            Some(ShellClass::Other) | None => return ShellClass::Other,
        }
    }

    if meaningful { class } else { ShellClass::Other }
}

fn strip_safe_redirections(command: &str) -> String {
    let mut text = command.to_string();
    for pattern in [
        "2>&1",
        "1>/dev/null",
        "2>/dev/null",
        "1>nul",
        "2>nul",
        "1>NUL",
        "2>NUL",
    ] {
        text = text.replace(pattern, " ");
    }
    text
}

fn has_single_ampersand(command: &str) -> bool {
    let bytes = command.as_bytes();
    for index in 0..bytes.len() {
        if bytes[index] != b'&' {
            continue;
        }
        let previous = index.checked_sub(1).map(|offset| bytes[offset]);
        let next = bytes.get(index + 1).copied();
        if previous != Some(b'&') && next != Some(b'&') {
            return true;
        }
    }
    false
}

fn split_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut chars = command.chars().peekable();
    while let Some(character) = chars.next() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            current.push(character);
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            current.push(character);
            if character == active {
                quote = None;
            }
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
            current.push(character);
            continue;
        }
        if character == ';'
            || character == '\n'
            || character == '|'
            || (character == '&' && chars.peek() == Some(&'&'))
        {
            if !current.trim().is_empty() {
                segments.push(current.trim().to_string());
            }
            current.clear();
            if character == '&' {
                chars.next();
            }
            continue;
        }
        current.push(character);
    }
    if !current.trim().is_empty() {
        segments.push(current.trim().to_string());
    }
    if quote.is_some() {
        return Vec::new();
    }
    segments
}

fn shell_words(segment: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for character in segment.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
            continue;
        }
        if character.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(character);
    }
    if escaped || quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        words.push(current);
    }
    Some(words)
}

fn classify_segment(raw: &str, args: &[String]) -> Option<ShellClass> {
    let base = raw
        .rsplit(|character| character == '/' || character == '\\')
        .next()
        .unwrap_or(raw)
        .to_ascii_lowercase();
    let command = base.strip_suffix(".exe").unwrap_or(&base);
    if command.is_empty() {
        return None;
    }
    if matches!(command, "echo" | "printf" | "true" | "tr" | "uniq") {
        return Some(ShellClass::ReadOnly);
    }
    match command {
        "cat" | "head" | "tail" | "wc" | "stat" | "file" | "tree" | "ls" | "dir" | "pwd"
        | "where" | "which" | "type" | "grep" | "rg" | "ack" | "ag" | "findstr" => {
            Some(ShellClass::ReadOnly)
        }
        "sed" => {
            if args.iter().any(|arg| {
                arg == "--in-place"
                    || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('i'))
            }) {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        "sort" => {
            if args
                .iter()
                .any(|arg| arg == "-o" || arg == "--output" || arg.starts_with("--output="))
            {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        "find" => {
            if args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "-exec" | "-execdir" | "-delete" | "-ok" | "-fprint" | "-fprintf" | "-fls"
                )
            }) {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        "git" => classify_git(args),
        "bash" | "sh" | "zsh" | "cmd" => {
            let script = args
                .iter()
                .position(|arg| matches!(arg.as_str(), "-c" | "-lc" | "--command" | "/c" | "/C"))
                .and_then(|index| args.get(index + 1));
            script.and_then(|value| match classify_shell(value) {
                ShellClass::Other => None,
                other => Some(other),
            })
        }
        "bun" => classify_bun(args),
        "cargo" => classify_cargo(args),
        "npm" => classify_npm(args),
        "pnpm" => classify_pnpm(args),
        "yarn" => classify_yarn(args),
        "npx" => classify_npx(args),
        "tsc" | "eslint" | "vitest" | "jest" => Some(ShellClass::Verify),
        "prettier" => {
            if args.iter().any(|arg| arg == "--check" || arg == "-c") {
                Some(ShellClass::Verify)
            } else {
                None
            }
        }
        "go" => match args.first().map(String::as_str) {
            Some("test") => Some(ShellClass::Verify),
            _ => None,
        },
        "pytest" => Some(ShellClass::Verify),
        "python" | "python3" => match (args.first().map(String::as_str), args.get(1)) {
            (Some("-m"), Some(module)) if module == "pytest" => Some(ShellClass::Verify),
            _ => None,
        },
        "deno" => match args.first().map(String::as_str) {
            Some("test" | "check" | "lint") => Some(ShellClass::Verify),
            _ => None,
        },
        _ => None,
    }
}

fn classify_git(args: &[String]) -> Option<ShellClass> {
    let subcommand = args.first()?.to_ascii_lowercase();
    match subcommand.as_str() {
        "status" | "log" | "show" | "rev-parse" | "describe" | "ls-files" | "grep" => {
            Some(ShellClass::ReadOnly)
        }
        "diff" => {
            if args
                .iter()
                .any(|arg| arg == "--output" || arg.starts_with("--output="))
            {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        "branch" => {
            if args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "-d" | "-D"
                        | "-m"
                        | "-M"
                        | "-f"
                        | "-c"
                        | "-C"
                        | "--delete"
                        | "--move"
                        | "--copy"
                        | "--force"
                )
            }) {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        _ => None,
    }
}

fn is_verify_script(script: &str) -> bool {
    VERIFY_SCRIPTS.contains(&script)
}

fn classify_bun(args: &[String]) -> Option<ShellClass> {
    match args.first().map(String::as_str) {
        Some("test") => Some(ShellClass::Verify),
        Some("run") => match args.get(1) {
            Some(script) if is_verify_script(script) => Some(ShellClass::Verify),
            _ => None,
        },
        Some("x") => classify_npx(&args[1..]),
        _ => None,
    }
}

fn classify_cargo(args: &[String]) -> Option<ShellClass> {
    match args.first().map(String::as_str) {
        Some("test" | "check" | "build" | "clippy" | "fmt") => Some(ShellClass::Verify),
        _ => None,
    }
}

fn classify_npm(args: &[String]) -> Option<ShellClass> {
    match args.first().map(String::as_str) {
        Some("test") => Some(ShellClass::Verify),
        Some("run") => match args.get(1) {
            Some(script) if is_verify_script(script) => Some(ShellClass::Verify),
            _ => None,
        },
        _ => None,
    }
}

fn classify_pnpm(args: &[String]) -> Option<ShellClass> {
    match args.first().map(String::as_str) {
        Some("test") => Some(ShellClass::Verify),
        Some("run") => match args.get(1) {
            Some(script) if is_verify_script(script) => Some(ShellClass::Verify),
            _ => None,
        },
        Some("exec") => classify_npx(&args[1..]),
        _ => None,
    }
}

fn classify_yarn(args: &[String]) -> Option<ShellClass> {
    match args.first().map(String::as_str) {
        Some("test") => Some(ShellClass::Verify),
        Some("run") => match args.get(1) {
            Some(script) if is_verify_script(script) => Some(ShellClass::Verify),
            _ => None,
        },
        _ => None,
    }
}

fn classify_npx(args: &[String]) -> Option<ShellClass> {
    match args.first().map(String::as_str) {
        Some("tsc" | "eslint" | "vitest" | "jest") => Some(ShellClass::Verify),
        Some("prettier") => {
            if args.iter().any(|arg| arg == "--check" || arg == "-c") {
                Some(ShellClass::Verify)
            } else {
                None
            }
        }
        _ => None,
    }
}

impl PolicyPort for DefaultPolicyPlugin {
    fn evaluate(&self, input: &PolicyInput) -> PolicyDecision {
        match input.tool_id.as_str() {
            "fs_list" | "fs_read" => match self.inside(input) {
                Ok(()) => PolicyDecision {
                    effect: PolicyEffect::Allow,
                    rationale: i18n::t("Read-only access inside the workspace"),
                },
                Err(reason) => PolicyDecision {
                    effect: PolicyEffect::Deny,
                    rationale: reason,
                },
            },
            "fs_write" => match self.inside(input) {
                Ok(()) => PolicyDecision {
                    effect: PolicyEffect::Allow,
                    rationale: i18n::t(
                        "Workspace writes are reversible and backed up; no approval required",
                    ),
                },
                Err(reason) => PolicyDecision {
                    effect: PolicyEffect::Deny,
                    rationale: reason,
                },
            },
            "shell_exec" => {
                let command = input
                    .args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if command.trim().is_empty() {
                    return PolicyDecision {
                        effect: PolicyEffect::Deny,
                        rationale: i18n::t("Empty command"),
                    };
                }
                let lowered = command.to_lowercase();
                if DANGEROUS_PATTERNS
                    .iter()
                    .any(|pattern| lowered.contains(pattern))
                {
                    return PolicyDecision {
                        effect: PolicyEffect::Deny,
                        rationale: i18n::t(
                            "Command matches a dangerous pattern; denied by default",
                        ),
                    };
                }
                match classify_shell(command) {
                    ShellClass::ReadOnly => PolicyDecision {
                        effect: PolicyEffect::Allow,
                        rationale: i18n::t(
                            "Read-only command inside the workspace; no approval required",
                        ),
                    },
                    ShellClass::Verify => PolicyDecision {
                        effect: PolicyEffect::Allow,
                        rationale: i18n::t("Project verification command; no approval required"),
                    },
                    ShellClass::Other => PolicyDecision {
                        effect: PolicyEffect::Ask,
                        rationale: i18n::t(
                            "Command is not on the safe whitelist; human approval is required",
                        ),
                    },
                }
            }
            "memory_search" => PolicyDecision {
                effect: PolicyEffect::Allow,
                rationale: i18n::t("Reading long-term memory does not touch the workspace"),
            },
            "memory_write" => PolicyDecision {
                effect: PolicyEffect::Allow,
                rationale: i18n::t(
                    "Memory writes pass the admission barrier and can be forgotten later",
                ),
            },
            "delegate" | "wait_for_agents" | "send_message" | "cancel_agent" => PolicyDecision {
                effect: PolicyEffect::Allow,
                rationale: i18n::t(
                    "Agent coordination does not touch the workspace; each subagent action is still policy-checked",
                ),
            },
            "image_generate" => PolicyDecision {
                effect: PolicyEffect::Allow,
                rationale: i18n::t(
                    "Image generation is an external service call; the image configuration and egress grant gate it",
                ),
            },
            other => PolicyDecision {
                effect: PolicyEffect::Deny,
                rationale: i18n::t_args(
                    "No policy matches {tool}; denied by default",
                    &[("tool", other.to_string())],
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::FsSandboxPlugin;
    use serde_json::Value;
    use serde_json::json;
    use std::path::PathBuf;

    fn ws() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("phi-policy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn policy() -> DefaultPolicyPlugin {
        DefaultPolicyPlugin::new(Arc::new(FsSandboxPlugin))
    }

    fn input(tool: &str, args: Value) -> PolicyInput {
        PolicyInput {
            tool_id: tool.to_string(),
            args,
            workspace: ws(),
        }
    }

    #[test]
    fn read_inside_is_allowed() {
        let d = policy().evaluate(&input("fs_read", json!({"path": "a.txt"})));
        assert_eq!(d.effect, PolicyEffect::Allow);
    }

    #[test]
    fn read_outside_is_denied() {
        let d = policy().evaluate(&input("fs_read", json!({"path": "../a.txt"})));
        assert_eq!(d.effect, PolicyEffect::Deny);
    }

    #[test]
    fn write_inside_is_allowed() {
        let d = policy().evaluate(&input("fs_write", json!({"path": "a.txt", "content": "x"})));
        assert_eq!(d.effect, PolicyEffect::Allow);
    }

    #[test]
    fn read_only_shell_is_allowed() {
        for command in [
            "echo hi",
            "ls -la",
            "rg TODO src",
            "git status",
            "cat a.txt | head -5",
        ] {
            let d = policy().evaluate(&input("shell_exec", json!({"command": command})));
            assert_eq!(d.effect, PolicyEffect::Allow, "command: {command}");
        }
    }

    #[test]
    fn verification_shell_is_allowed() {
        for command in ["cargo test", "cargo check", "bun run build", "npm test"] {
            let d = policy().evaluate(&input("shell_exec", json!({"command": command})));
            assert_eq!(d.effect, PolicyEffect::Allow, "command: {command}");
        }
    }

    #[test]
    fn unknown_shell_asks() {
        for command in ["python script.py", "rm -rf ./dist", "git branch -D topic"] {
            let d = policy().evaluate(&input("shell_exec", json!({"command": command})));
            assert_eq!(d.effect, PolicyEffect::Ask, "command: {command}");
        }
    }

    #[test]
    fn redirection_and_substitution_ask() {
        for command in ["cat a.txt > b.txt", "echo $(rm x)", "ls `pwd`"] {
            let d = policy().evaluate(&input("shell_exec", json!({"command": command})));
            assert_eq!(d.effect, PolicyEffect::Ask, "command: {command}");
        }
    }

    #[test]
    fn destructive_shell_is_denied() {
        let d = policy().evaluate(&input("shell_exec", json!({"command": "rm -rf /"})));
        assert_eq!(d.effect, PolicyEffect::Deny);
    }

    #[test]
    fn unknown_tool_is_denied() {
        let d = policy().evaluate(&input("whatever", json!({})));
        assert_eq!(d.effect, PolicyEffect::Deny);
    }
}
