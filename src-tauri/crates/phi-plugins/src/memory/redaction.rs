//! Write-side secret redaction.
//!
//! User input, tool output and model output can all contain credentials. Every
//! fact and episode passes through these rules before persistence. The rules
//! are deliberately conservative: a false positive only loses a token, while a
//! missed secret is stored forever.
//!
//! This is pattern matching, not entropy analysis. Unknown secret formats are
//! still covered by the source-trust rule: unclassified content is stored with
//! a low trust level and is never treated as confirmed knowledge.

use std::sync::OnceLock;

use regex::Regex;

struct Rule {
    pattern: Regex,
    replacement: &'static str,
}

fn rules() -> &'static Vec<Rule> {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let rule = |pattern: &str, replacement: &'static str| Rule {
            pattern: Regex::new(pattern).expect("redaction pattern must compile"),
            replacement,
        };
        vec![
            rule(
                r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
                "[REDACTED:private-key]",
            ),
            rule(
                r"(?i)\b([a-z][a-z0-9+.-]*://[^:/@\s]+):([^@\s/]+)@",
                "${1}:[REDACTED:password]@",
            ),
            rule(r"\bsk-(?:ant-)?[A-Za-z0-9_-]{16,}\b", "[REDACTED:api-key]"),
            rule(r"\bAKIA[0-9A-Z]{16}\b", "[REDACTED:aws-key]"),
            rule(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b", "[REDACTED:github-token]"),
            rule(r"\bxox[abprs]-[A-Za-z0-9-]{10,}\b", "[REDACTED:slack-token]"),
            rule(
                r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b",
                "[REDACTED:jwt]",
            ),
            rule(
                r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{16,}",
                "Bearer [REDACTED:token]",
            ),
            rule(
                r#"(?i)\b([a-z0-9_-]*(?:api[_-]?key|secret|token|password|passwd|pwd|access[_-]?key)[a-z0-9_-]*)\s*[:=]\s*("[^"\n]{6,}"|'[^'\n]{6,}'|[a-z0-9._~+/=!@#$%^&*()-]{8,})"#,
                "${1}=[REDACTED:secret]",
            ),
        ]
    })
}

/// Result of redaction. `only_secret` means nothing but markers and punctuation
/// remain, so there is no fact worth keeping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redacted {
    pub text: String,
    pub redacted: bool,
    pub only_secret: bool,
}

pub fn redact_secrets(input: &str) -> Redacted {
    let mut text = input.to_string();
    for rule in rules() {
        if rule.pattern.is_match(&text) {
            text = rule
                .pattern
                .replace_all(&text, rule.replacement)
                .into_owned();
        }
    }
    if text == input {
        return Redacted {
            text,
            redacted: false,
            only_secret: false,
        };
    }
    static MARKER: OnceLock<Regex> = OnceLock::new();
    let marker = MARKER.get_or_init(|| {
        Regex::new(r"\[REDACTED:[a-z-]+\]").expect("redaction marker must compile")
    });
    let remaining = marker.replace_all(&text, "");
    let only_secret = !remaining.chars().any(|ch| ch.is_alphanumeric());
    Redacted {
        text,
        redacted: true,
        only_secret,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_ordinary_text_alone() {
        let result = redact_secrets("The project uses pnpm and Rust 2024.");
        assert!(!result.redacted);
        assert_eq!(result.text, "The project uses pnpm and Rust 2024.");
    }

    #[test]
    fn redacts_api_keys_inside_a_fact() {
        let result = redact_secrets("Use the staging key sk-test0000000000000000 for now.");
        assert!(result.redacted);
        assert!(!result.text.contains("sk-test"));
        assert!(result.text.contains("[REDACTED:api-key]"));
        assert!(!result.only_secret);
    }

    #[test]
    fn a_bare_secret_is_recognized() {
        let result = redact_secrets("sk-test0000000000000000");
        assert!(result.redacted);
        assert!(result.only_secret);
    }

    #[test]
    fn redacts_private_key_blocks() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow\n-----END RSA PRIVATE KEY-----";
        let result = redact_secrets(input);
        assert_eq!(result.text, "[REDACTED:private-key]");
        assert!(result.only_secret);
    }

    #[test]
    fn redacts_connection_string_passwords() {
        let result = redact_secrets("DATABASE_URL=postgres://app:hunter2pass@db.internal/app");
        assert!(result.redacted);
        assert!(!result.text.contains("hunter2pass"));
        assert!(result.text.contains("postgres://app:[REDACTED:password]@"));
    }

    #[test]
    fn redacts_key_value_secrets() {
        let result = redact_secrets("api_key: abcdef12345678");
        assert!(result.redacted);
        assert!(result.text.contains("[REDACTED:secret]"));
    }

    #[test]
    fn does_not_redact_env_references_or_placeholders() {
        for input in [
            "OPENAI_API_KEY=env:OPENAI_API_KEY",
            "password: ${DB_PASSWORD}",
            "token: <your-token>",
            "password must be at least 8 characters",
        ] {
            assert!(
                !redact_secrets(input).redacted,
                "unexpected redaction: {input}"
            );
        }
    }

    #[test]
    fn redacts_bearer_tokens() {
        let result = redact_secrets("Authorization: Bearer abcdefghijklmnop123456");
        assert!(result.redacted);
        assert!(result.text.contains("Bearer [REDACTED:token]"));
    }
}
