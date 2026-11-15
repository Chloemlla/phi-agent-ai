//! Starter model metadata and name-based auto-configuration.
//!
//! The data lives in `assets/model-catalog.json` and is compiled into the
//! binary. It is a convenience only: every value can be overridden in the
//! settings UI, and the provider API can supply the live model list. An empty
//! catalogue leaves the model as a manual spec with only its id.
//!
//! Entry fields: `patterns` (lower-case substrings matched against the model id
//! after the vendor prefix is stripped; any one may match) and `requires`
//! (every substring must also match, which separates variants such as
//! `-flash` or `-pro`). `display` is a template with `{version}`; the version
//! is read from the digits following `anchor` in the model id, so a new point
//! release such as `gpt-6.5-luna` still shows its real version. `name` is the
//! fallback when no version can be read. `scale` ranks capability from 1 to 5
//! for delegation caps.

use std::sync::OnceLock;

use serde::Deserialize;

use phi_contracts::provider::ModelSpec;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CatalogEntry {
    patterns: Vec<String>,
    requires: Vec<String>,
    name: String,
    display: Option<String>,
    anchor: Option<String>,
    description: String,
    tags: Vec<String>,
    context_window: Option<u64>,
    max_output_tokens: Option<u32>,
    scale: u8,
}

fn entries() -> &'static [CatalogEntry] {
    static ENTRIES: OnceLock<Vec<CatalogEntry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let raw = include_str!("../../../assets/model-catalog.json");
        serde_json::from_str(raw).expect("assets/model-catalog.json must be valid JSON")
    })
}

fn normalized(id: &str) -> String {
    let lower = id.trim().to_ascii_lowercase();
    // OpenRouter-style ids prefix the vendor; match on the model part.
    lower.rsplit('/').next().unwrap_or(&lower).to_string()
}

fn match_entry<'a>(entries: &'a [CatalogEntry], id: &str) -> Option<&'a CatalogEntry> {
    let id = normalized(id);
    let mut best: Option<(usize, &CatalogEntry)> = None;
    for entry in entries {
        let mut longest = 0;
        for pattern in &entry.patterns {
            let pattern = pattern.to_ascii_lowercase();
            if id.contains(&pattern) {
                longest = longest.max(pattern.len());
            }
        }
        if longest == 0
            || !entry
                .requires
                .iter()
                .all(|needle| id.contains(&needle.to_ascii_lowercase()))
        {
            continue;
        }
        // A longer pattern and more required markers mean a more specific
        // entry; the most specific match wins regardless of file order.
        let score = longest * 100
            + entry
                .requires
                .iter()
                .map(|needle| needle.len())
                .sum::<usize>();
        if best.map_or(true, |(best_score, _)| score > best_score) {
            best = Some((score, entry));
        }
    }
    best.map(|(_, entry)| entry)
}

/// Read a dotted version after `anchor`, treating `-` and `.` alike so both
/// `claude-sonnet-5-5` and `claude-sonnet-5.5` display as 5.5. At most two
/// numeric parts are kept, which drops dated snapshots such as
/// `claude-sonnet-5-5-20260101`.
fn version_after(id: &str, anchor: &str) -> Option<String> {
    let lower = id.to_ascii_lowercase();
    let start = lower.find(&anchor.to_ascii_lowercase())? + anchor.len();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in lower[start..].chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if ch == '.' || ch == '-' {
            if current.is_empty() {
                break;
            }
            parts.push(std::mem::take(&mut current));
            if parts.len() == 2 {
                break;
            }
        } else {
            break;
        }
    }
    if !current.is_empty() && parts.len() < 2 {
        parts.push(current);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

fn display_name(entry: &CatalogEntry, id: &str) -> String {
    if let (Some(anchor), Some(template)) = (entry.anchor.as_deref(), entry.display.as_deref()) {
        if let Some(version) = version_after(id, anchor) {
            return template.replace("{version}", &version);
        }
    }
    if entry.name.is_empty() {
        id.to_string()
    } else {
        entry.name.clone()
    }
}

fn describe_with(entries: &[CatalogEntry], id: &str) -> ModelSpec {
    let id = id.trim();
    let Some(entry) = match_entry(entries, id) else {
        return ModelSpec::from_id(id);
    };
    ModelSpec {
        id: id.to_string(),
        name: display_name(entry, id),
        description: entry.description.clone(),
        tags: entry.tags.clone(),
        context_window: entry.context_window,
        max_output_tokens: entry.max_output_tokens,
        scale: if entry.scale == 0 {
            3
        } else {
            entry.scale.clamp(1, 5)
        },
        temperature: None,
        enabled: true,
        source: "catalog".to_string(),
    }
}

/// Best-effort metadata for a model id. Never fails: an unknown id comes back
/// as a manual spec with only the id filled in.
pub fn describe(id: &str) -> ModelSpec {
    describe_with(entries(), id)
}

pub fn describe_many(ids: &[String]) -> Vec<ModelSpec> {
    ids.iter().map(|id| describe(id)).collect()
}

fn enrich_with(entries: &[CatalogEntry], mut spec: ModelSpec) -> ModelSpec {
    spec.normalize();
    if spec.id.is_empty() {
        return spec;
    }
    let suggested = describe_with(entries, &spec.id);
    if suggested.source != "catalog" {
        return spec;
    }
    let manual = spec.source == "manual";
    let mut filled = false;
    if spec.name.is_empty() || spec.name == spec.id {
        spec.name = suggested.name;
        filled = true;
    }
    if spec.description.is_empty() {
        spec.description = suggested.description;
        filled = true;
    }
    if spec.tags.is_empty() {
        spec.tags = suggested.tags;
        filled = true;
    }
    if spec.scale == 3 && suggested.scale != 3 {
        // Scale is capability metadata; 3 is the unknown default, so a
        // catalogue entry may replace it even for a manual spec.
        spec.scale = suggested.scale;
        filled = true;
    }
    if !manual {
        if spec.context_window.is_none() {
            spec.context_window = suggested.context_window;
            filled = spec.context_window.is_some() || filled;
        }
        if spec.max_output_tokens.is_none() {
            spec.max_output_tokens = suggested.max_output_tokens;
            filled = spec.max_output_tokens.is_some() || filled;
        }
    }
    if filled && !manual {
        spec.source = "catalog".to_string();
    }
    spec
}

/// Fill only the metadata the caller left empty. Explicit values always win,
/// so a hand-edited description survives a re-save. A spec marked `manual`
/// keeps its numeric fields untouched: clearing a limit is how the user asks
/// to inherit the provider default.
pub fn enrich(spec: ModelSpec) -> ModelSpec {
    enrich_with(entries(), spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<CatalogEntry> {
        serde_json::from_str(
            r#"[
                {
                    "patterns": ["claude-sonnet"],
                    "name": "Claude Sonnet",
                    "description": "Coding model.",
                    "tags": ["coding"],
                    "contextWindow": 200000,
                    "maxOutputTokens": 64000
                },
                {
                    "patterns": ["gpt-5-mini"],
                    "name": "GPT-5 mini"
                },
                {
                    "patterns": ["gpt-5"],
                    "name": "GPT-5"
                }
            ]"#,
        )
        .unwrap()
    }

    #[test]
    fn shipped_catalog_is_valid_json() {
        // A malformed asset would otherwise panic lazily at the first lookup.
        let _ = entries();
    }

    #[test]
    fn shipped_catalog_maps_current_model_families() {
        // An unrecorded point release still resolves to the right variant and
        // shows its real version.
        let luna = describe("gpt-6.2-luna");
        assert_eq!(luna.name, "GPT-6.2 Luna");
        assert_eq!(luna.context_window, Some(1_050_000));
        assert_eq!(luna.max_output_tokens, Some(128_000));
        assert_eq!(luna.scale, 2);

        let astra = describe("gpt-6.5-astra");
        assert_eq!(astra.name, "GPT-6.5 Astra");
        assert_eq!(astra.scale, 5);

        let sonnet = describe("anthropic/claude-sonnet-5-5");
        assert_eq!(sonnet.name, "Claude Sonnet 5.5");
        assert_eq!(sonnet.context_window, Some(1_000_000));

        let fable = describe("claude-fable-5-2");
        assert_eq!(fable.name, "Claude Fable 5.2");
        assert_eq!(fable.scale, 5);

        let flash = describe("glm-5.3-flash");
        assert_eq!(flash.name, "GLM-5.3-Flash");
        assert_eq!(flash.scale, 3);

        let deepseek = describe("deepseek-v4.1-flash");
        assert_eq!(deepseek.max_output_tokens, Some(393_216));

        let argon = describe("gemini-4-argon");
        assert_eq!(argon.name, "Gemini 4 Argon");
        assert_eq!(argon.max_output_tokens, Some(1_048_576));
    }

    #[test]
    fn versions_ignore_dated_snapshots() {
        let spec = describe("claude-opus-5-5-20260101");
        assert_eq!(spec.name, "Claude Opus 5.5");
        assert_eq!(
            version_after("claude-opus-5-5", "claude-opus-"),
            Some("5.5".into())
        );
        assert_eq!(version_after("gpt-6.5-luna", "gpt-"), Some("6.5".into()));
        assert_eq!(version_after("no-version-here", "gpt-"), None);
    }

    #[test]
    fn matches_vendor_prefixed_ids() {
        let spec = describe_with(&fixture(), "anthropic/claude-sonnet-4.5");
        assert_eq!(spec.source, "catalog");
        assert_eq!(spec.name, "Claude Sonnet");
        assert!(spec.tags.contains(&"coding".to_string()));
        assert_eq!(spec.context_window, Some(200_000));
    }

    #[test]
    fn specific_variants_win_over_generic_ones() {
        assert_eq!(describe_with(&fixture(), "gpt-5-mini").name, "GPT-5 mini");
        assert_eq!(describe_with(&fixture(), "gpt-5").name, "GPT-5");
    }

    #[test]
    fn unknown_models_stay_manual() {
        let spec = describe_with(&[], "my-private-model");
        assert_eq!(spec.source, "manual");
        assert_eq!(spec.id, "my-private-model");
        assert!(spec.description.is_empty());
    }

    #[test]
    fn enrich_keeps_explicit_values() {
        let mut spec = ModelSpec::from_id("claude-sonnet-4.5");
        spec.description = "Custom description".to_string();
        spec.max_output_tokens = Some(12_345);
        spec.scale = 5;
        let enriched = enrich_with(&fixture(), spec);
        assert_eq!(enriched.description, "Custom description");
        assert_eq!(enriched.max_output_tokens, Some(12_345));
        assert_eq!(enriched.name, "Claude Sonnet");
        assert_eq!(enriched.scale, 5);
    }
}
