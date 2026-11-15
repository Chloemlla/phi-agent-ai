//! Admission barrier: deterministic checks that run before a candidate becomes
//! a `MemoryRecord`. LLM-assisted admission is a later stage; these rules are
//! the part that must never be skipped.

use chrono::{DateTime, SecondsFormat, Utc};

use phi_domain::memory::{AdmissionRejection, has_meaningful_content};

use super::redaction::redact_secrets;

pub const MAX_FACT_CHARS: usize = 2000;
pub const MAX_EPISODE_CHARS: usize = 8000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFact {
    pub fact: String,
    pub redacted: bool,
}

/// Normalize, bound and redact a candidate fact.
pub fn prepare_fact(raw: &str, max_chars: usize) -> Result<PreparedFact, AdmissionRejection> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !has_meaningful_content(trimmed) {
        return Err(AdmissionRejection::Empty);
    }
    if trimmed.chars().count() > max_chars {
        return Err(AdmissionRejection::TooLong);
    }
    let redacted = redact_secrets(trimmed);
    if redacted.only_secret {
        return Err(AdmissionRejection::Secret);
    }
    if !has_meaningful_content(&redacted.text) {
        return Err(AdmissionRejection::Empty);
    }
    // Facts are atomic, single-line statements. Collapsing whitespace keeps
    // retrieval and prompt injection predictable.
    let fact = redacted
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(PreparedFact {
        fact,
        redacted: redacted.redacted,
    })
}

/// Canonicalize a caller-supplied timestamp to UTC milliseconds. Lexical order
/// on canonical strings equals chronological order.
pub fn canonical_time(raw: &str) -> Result<String, AdmissionRejection> {
    DateTime::parse_from_rfc3339(raw.trim())
        .map(|time| {
            time.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true)
        })
        .map_err(|_| AdmissionRejection::InvalidValidity)
}

pub fn validate_validity(
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(Option<String>, Option<String>), AdmissionRejection> {
    let from = from.map(canonical_time).transpose()?;
    let to = to.map(canonical_time).transpose()?;
    if let (Some(from), Some(to)) = (&from, &to) {
        if from >= to {
            return Err(AdmissionRejection::InvalidValidity);
        }
    }
    Ok((from, to))
}

/// Redact and bound an episode before it is stored. The episode is raw source
/// material, not a fact, so the length bound is larger and truncation is
/// explicit.
pub fn prepare_episode(content: &str, max_chars: usize) -> (String, bool) {
    let redacted = redact_secrets(content.trim());
    let mut text = redacted.text;
    if text.chars().count() > max_chars {
        text = text.chars().take(max_chars).collect::<String>();
        text.push_str("...");
    }
    (text, redacted.redacted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_noise() {
        assert_eq!(prepare_fact("   ", 100), Err(AdmissionRejection::Empty));
        assert_eq!(prepare_fact(" --- ", 100), Err(AdmissionRejection::Empty));
    }

    #[test]
    fn rejects_overlong_facts() {
        let long = "a".repeat(101);
        assert_eq!(prepare_fact(&long, 100), Err(AdmissionRejection::TooLong));
    }

    #[test]
    fn rejects_a_bare_secret_but_keeps_a_redacted_fact() {
        assert_eq!(
            prepare_fact("sk-test0000000000000000", 200),
            Err(AdmissionRejection::Secret)
        );
        let prepared = prepare_fact("The staging key is sk-test0000000000000000", 200).unwrap();
        assert!(prepared.redacted);
        assert!(!prepared.fact.contains("sk-test"));
    }

    #[test]
    fn canonicalizes_offsets_for_lexical_comparison() {
        let from = canonical_time("2026-01-01T08:00:00+08:00").unwrap();
        let to = canonical_time("2026-01-02T00:00:00Z").unwrap();
        assert_eq!(from, "2026-01-01T00:00:00.000Z");
        assert!(from < to);
    }

    #[test]
    fn rejects_inverted_validity_windows() {
        assert_eq!(
            validate_validity(Some("2026-02-01T00:00:00Z"), Some("2026-01-01T00:00:00Z")),
            Err(AdmissionRejection::InvalidValidity)
        );
        assert!(validate_validity(None, Some("2026-01-01T00:00:00Z")).is_ok());
    }

    #[test]
    fn truncates_long_episodes_after_redaction() {
        let content = format!("{} sk-test0000000000000000", "x".repeat(50));
        let (text, redacted) = prepare_episode(&content, 40);
        assert!(redacted);
        assert!(!text.contains("sk-test"));
        assert!(text.ends_with("..."));
    }
}
