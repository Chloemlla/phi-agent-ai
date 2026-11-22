use std::sync::Arc;

use phi_contracts::SLOT_POLICY_EVALUATE;
use phi_contracts::ports::{
    PolicyDecision, PolicyEffect, PolicyInput, PolicyPort, SandboxPort, ToolCapability,
};
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

/// Shell arguments are resolved by the shell, not by the workspace sandbox.
/// Reject anything that can escape the workspace before classifying a command
/// as safe: absolute/drive/UNC paths, home expansion, explicit parent
/// traversal, and attached short-option paths such as `-o/tmp/out`.
fn argument_escapes_workspace(word: &str) -> bool {
    if word.starts_with('-') && (word.contains('/') || word.contains('\\')) {
        return true;
    }
    if path_escapes_workspace(word) {
        return true;
    }
    word.split_once('=')
        .is_some_and(|(_, value)| path_escapes_workspace(value))
}

/// Catch Windows separators and drive prefixes in the raw command. The token
/// parser follows Unix quoting rules on Unix hosts, which would otherwise
/// consume `\` as an escape and hide `..\secret` from the token check.
fn raw_command_escapes_workspace(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    if lower.contains("../") || lower.contains("..\\") || lower.contains("\\\\") {
        return true;
    }
    let bytes = lower.as_bytes();
    bytes
        .windows(2)
        .any(|pair| pair[0] == b':' && matches!(pair[1], b'/' | b'\\'))
}

fn path_escapes_workspace(value: &str) -> bool {
    if value.starts_with('~')
        || value.starts_with("\\\\")
        || (cfg!(windows) && value.starts_with('\\'))
    {
        return true;
    }
    if value.starts_with('/') && !(cfg!(windows) && is_windows_switch(value)) {
        return true;
    }
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    // `git show HEAD:/etc/passwd` and `HEAD:../../secret` are outside the
    // workspace even though they do not start with a slash.
    if value.contains(":/") || value.contains(":..") {
        return true;
    }
    value
        .split(|character| character == '/' || character == '\\')
        .any(|part| part == "..")
}

/// Windows commands accept `/b`, `/s` and `/a:d` as switches. Treat those as
/// options rather than Unix-style absolute paths.
fn is_windows_switch(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('/') else {
        return false;
    };
    let mut characters = rest.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let tail = characters.as_str();
    tail.is_empty()
        || (tail.starts_with(':')
            && tail[1..]
                .chars()
                .all(|character| character.is_ascii_alphanumeric()))
}

fn is_quote(character: char) -> bool {
    character == '"' || (!cfg!(windows) && character == '\'')
}

fn is_escape(character: char, quote: Option<char>) -> bool {
    character == '\\' && !cfg!(windows) && quote != Some('\'')
}

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
        || raw_command_escapes_workspace(&normalized)
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
        if args.iter().any(|arg| argument_escapes_workspace(arg)) {
            return ShellClass::Other;
        }
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
        if is_escape(character, quote) {
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
        if is_quote(character) {
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
        if is_escape(character, quote) {
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
        if is_quote(character) {
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
    // Only bare command names are safe: `/tmp/cat` or `.\cat` would execute a
    // binary chosen by the model while still matching the allowlist name.
    if raw.contains('/') || raw.contains('\\') {
        return None;
    }
    let base = raw.to_ascii_lowercase();
    let command = base.strip_suffix(".exe").unwrap_or(&base);
    if command.is_empty() {
        return None;
    }
    if matches!(command, "echo" | "printf" | "true" | "tr" | "uniq") {
        return Some(ShellClass::ReadOnly);
    }
    match command {
        "cat" | "head" | "tail" | "wc" | "stat" | "file" | "ls" | "dir" | "pwd" | "where"
        | "which" | "type" | "grep" | "findstr" => Some(ShellClass::ReadOnly),
        "tree" => {
            if has_option(args, &["-o", "--output"])
                || args
                    .iter()
                    .any(|arg| arg.starts_with("-o") && arg.len() > 2)
            {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        "rg" => {
            if has_option(
                args,
                &[
                    "--pre",
                    "--pre-glob",
                    "--search-zip",
                    "--hostname-bin",
                    "--pager",
                ],
            ) || args
                .iter()
                .any(|arg| arg.starts_with('-') && !arg.starts_with("--") && arg.contains('z'))
            {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        "ack" | "ag" => {
            if has_option(args, &["--pager", "--pager-program"]) {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        // sed scripts can execute commands through the `e` command; there is
        // no reliable way to validate a script, so it always asks.
        "sed" => None,
        "sort" => {
            if has_option(
                args,
                &[
                    "-o",
                    "--output",
                    "--compress-program",
                    "--random-source",
                    "--files0-from",
                    "-T",
                    "--temporary-directory",
                ],
            ) || args
                .iter()
                .any(|arg| arg.starts_with("-o") && arg.len() > 2)
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
                    "-exec"
                        | "-execdir"
                        | "-ok"
                        | "-okdir"
                        | "-delete"
                        | "-fprint"
                        | "-fprintf"
                        | "-fls"
                        | "-fprint0"
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
    let rest = &args[1..];
    // Git can delegate to external diff/textconv drivers and pagers. Any of
    // these turns an otherwise read-only subcommand into code execution.
    if rest.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--ext-diff" | "--textconv" | "--open-files-in-pager"
        ) || arg.starts_with("--open-files-in-pager=")
            || arg == "-O"
            || (arg.starts_with("-O") && arg.len() > 2)
    }) {
        return None;
    }
    match subcommand.as_str() {
        "status" | "log" | "show" | "rev-parse" | "describe" | "ls-files" | "grep" => {
            Some(ShellClass::ReadOnly)
        }
        "diff" => {
            if has_option(rest, &["--output"]) {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        "branch" => {
            if rest.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "-d" | "-D"
                        | "-m"
                        | "-M"
                        | "-f"
                        | "-c"
                        | "-C"
                        | "-u"
                        | "--delete"
                        | "--move"
                        | "--copy"
                        | "--force"
                        | "--edit-description"
                        | "--unset-upstream"
                        | "--track"
                        | "--no-track"
                ) || (arg.starts_with("-u") && arg.len() > 2)
            }) || has_option(rest, &["--set-upstream-to"])
            {
                None
            } else {
                Some(ShellClass::ReadOnly)
            }
        }
        _ => None,
    }
}

fn has_option(args: &[String], options: &[&str]) -> bool {
    args.iter().any(|arg| {
        options.iter().any(|option| {
            arg == option
                || arg
                    .strip_prefix(option)
                    .is_some_and(|rest| rest.starts_with('=') || rest.starts_with(':'))
        })
    })
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

impl DefaultPolicyPlugin {
    fn evaluate_shell(&self, input: &PolicyInput) -> PolicyDecision {
        let command = input
            .args
            .get("command")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if command.trim().is_empty() {
            return deny(i18n::t("Empty command"));
        }
        let lowered = command.to_lowercase();
        if DANGEROUS_PATTERNS
            .iter()
            .any(|pattern| lowered.contains(pattern))
        {
            return deny(i18n::t(
                "Command matches a dangerous pattern; denied by default",
            ));
        }
        match classify_shell(command) {
            ShellClass::ReadOnly => allow(i18n::t(
                "Read-only command inside the workspace; no approval required",
            )),
            ShellClass::Verify => allow(i18n::t(
                "Project verification command; no approval required",
            )),
            ShellClass::Other => PolicyDecision {
                effect: PolicyEffect::Ask,
                rationale: i18n::t(
                    "Command is not on the safe whitelist; human approval is required",
                ),
            },
        }
    }
}

fn allow(rationale: String) -> PolicyDecision {
    PolicyDecision {
        effect: PolicyEffect::Allow,
        rationale,
    }
}

fn deny(rationale: String) -> PolicyDecision {
    PolicyDecision {
        effect: PolicyEffect::Deny,
        rationale,
    }
}

impl PolicyPort for DefaultPolicyPlugin {
    fn evaluate(&self, input: &PolicyInput) -> PolicyDecision {
        let has = |capability| input.capabilities.contains(&capability);

        // Order matters: the most dangerous capability wins when a tool
        // declares more than one.
        if has(ToolCapability::Shell) {
            return self.evaluate_shell(input);
        }
        if has(ToolCapability::WorkspaceWrite) {
            return match self.inside(input) {
                Ok(()) => allow(i18n::t(
                    "Workspace writes are reversible and backed up; no approval required",
                )),
                Err(reason) => deny(reason),
            };
        }
        if has(ToolCapability::WorkspaceRead) {
            return match self.inside(input) {
                Ok(()) => allow(i18n::t("Read-only access inside the workspace")),
                Err(reason) => deny(reason),
            };
        }
        if has(ToolCapability::MemoryWrite) {
            return allow(i18n::t(
                "Memory writes pass the admission barrier and can be forgotten later",
            ));
        }
        if has(ToolCapability::MemoryRead) {
            return allow(i18n::t(
                "Reading long-term memory does not touch the workspace",
            ));
        }
        if has(ToolCapability::ImageGeneration) {
            return allow(i18n::t(
                "Image generation is an external service call; the image configuration and egress grant gate it",
            ));
        }
        if has(ToolCapability::AgentControl) || has(ToolCapability::AgentMessaging) {
            return allow(i18n::t(
                "Agent coordination does not touch the workspace; each subagent action is still policy-checked",
            ));
        }
        if has(ToolCapability::ExternalEgress) {
            return allow(i18n::t(
                "External service call; the endpoint configuration and egress grant gate it",
            ));
        }
        deny(i18n::t_args(
            "No policy matches {tool}; denied by default",
            &[("tool", input.tool_id.clone())],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::FsSandboxPlugin;
    use phi_contracts::ports::SideEffect;
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
        let capabilities = match tool {
            "fs_list" | "fs_read" => vec![ToolCapability::WorkspaceRead],
            "fs_write" => vec![ToolCapability::WorkspaceWrite],
            "shell_exec" => vec![ToolCapability::Shell],
            "memory_search" => vec![ToolCapability::MemoryRead],
            "memory_write" => vec![ToolCapability::MemoryWrite],
            "delegate" | "wait_for_agents" | "send_message" | "cancel_agent" => {
                vec![ToolCapability::AgentControl]
            }
            "image_generate" => vec![
                ToolCapability::ExternalEgress,
                ToolCapability::ImageGeneration,
            ],
            _ => Vec::new(),
        };
        PolicyInput {
            tool_id: tool.to_string(),
            args,
            workspace: ws(),
            capabilities,
            side_effect: SideEffect::None,
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
    fn shell_paths_outside_the_workspace_ask() {
        for command in [
            "cat /etc/passwd",
            "cat ~/.ssh/id_rsa",
            "rg API_KEY ~",
            "cat ..\\secret.txt",
            "cat C:\\Windows\\win.ini",
            "cat src/../../secret",
            "git show HEAD:/etc/passwd",
            "git show HEAD:../../secret",
            "sort --output=/tmp/out file",
            "sort -o/tmp/out file",
            "cat --file=/etc/passwd",
        ] {
            let d = policy().evaluate(&input("shell_exec", json!({"command": command})));
            assert_eq!(d.effect, PolicyEffect::Ask, "command: {command}");
        }
    }

    #[test]
    fn exec_capable_read_only_commands_ask() {
        for command in [
            "rg --pre 'sh -c id' pattern .",
            "rg -z pattern .",
            "sed -n '1e id' file",
            "sort --compress-program=sh file",
            "git grep -O id pattern",
            "git grep --open-files-in-pager=less pattern",
            "git diff --ext-diff",
            "git log --textconv",
            "tree -o /tmp/out .",
            "ack --pager rg pattern",
            "/tmp/cat a.txt",
            "./cat a.txt",
        ] {
            let d = policy().evaluate(&input("shell_exec", json!({"command": command})));
            assert_eq!(d.effect, PolicyEffect::Ask, "command: {command}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_root_relative_paths_ask() {
        let d = policy().evaluate(&input("shell_exec", json!({"command": "cat \\secret.txt"})));
        assert_eq!(d.effect, PolicyEffect::Ask);
    }

    #[cfg(windows)]
    #[test]
    fn windows_switches_are_not_mistaken_for_paths() {
        let d = policy().evaluate(&input("shell_exec", json!({"command": "dir /b"})));
        assert_eq!(d.effect, PolicyEffect::Allow);
    }

    #[test]
    fn structured_read_only_commands_still_allow() {
        for command in [
            "cat a.txt",
            "rg TODO src",
            "git status",
            "git log --oneline -5",
            "git diff --stat",
            "git grep TODO",
            "find . -name '*.rs'",
            "sort file.txt",
            "tree src",
            "bash -c \"cargo test\"",
        ] {
            let d = policy().evaluate(&input("shell_exec", json!({"command": command})));
            assert_eq!(d.effect, PolicyEffect::Allow, "command: {command}");
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
