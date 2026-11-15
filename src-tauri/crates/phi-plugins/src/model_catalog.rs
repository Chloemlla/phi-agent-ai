//! Starter model metadata and name-based auto-configuration.
//!
//! The data lives in `assets/model-catalog.json` and is compiled into the
//! binary. It is a convenience only: every value can be overridden in the
//! settings UI, and the provider API can supply the live model list. An empty
//! catalogue leaves the model as a manual spec with only its id.
//!
//! Entry fields: `patterns` (lower-case substrings matched against the model id
//! after the vendor prefix is stripped), `name`, `description`, `tags`,
//! `contextWindow`, `maxOutputTokens` and `pricing` (`inputPerMtok` /
//! `outputPerMtok` / `cacheReadPerMtok` / `cacheWritePerMtok`).

use std::sync::OnceLock;

use serde::Deserialize;

use phi_contracts::provider::{ModelSpec, Pricing};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CatalogEntry {
    patterns: Vec<String>,
    name: String,
    description: String,
    tags: Vec<String>,
    context_window: Option<u64>,
    max_output_tokens: Option<u32>,
    pricing: Option<Pricing>,
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
    entries.iter().find(|entry| {
        entry
            .patterns
            .iter()
            .any(|pattern| id.contains(&pattern.to_ascii_lowercase()))
    })
}

fn describe_with(entries: &[CatalogEntry], id: &str) -> ModelSpec {
    let id = id.trim();
    let Some(entry) = match_entry(entries, id) else {
        return ModelSpec::from_id(id);
    };
    ModelSpec {
        id: id.to_string(),
        name: entry.name.clone(),
        description: entry.description.clone(),
        tags: entry.tags.clone(),
        context_window: entry.context_window,
        max_output_tokens: entry.max_output_tokens,
        pricing: entry.pricing,
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
    if !manual {
        if spec.context_window.is_none() {
            spec.context_window = suggested.context_window;
            filled = spec.context_window.is_some() || filled;
        }
        if spec.max_output_tokens.is_none() {
            spec.max_output_tokens = suggested.max_output_tokens;
            filled = spec.max_output_tokens.is_some() || filled;
        }
        if spec.pricing.is_none() {
            spec.pricing = suggested.pricing;
            filled = spec.pricing.is_some() || filled;
        }
    }
    if filled && !manual {
        spec.source = "catalog".to_string();
    }
    spec
}

/// Fill only the metadata the caller left empty. Explicit values always win,
/// so a hand-edited price or description survives a re-save. A spec marked
/// `manual` keeps its numeric fields untouched: clearing a price or limit is
/// how the user asks to inherit the provider default.
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
                    "maxOutputTokens": 64000,
                    "pricing": {
                        "inputPerMtok": 3.0,
                        "outputPerMtok": 15.0,
                        "cacheReadPerMtok": 0.3,
                        "cacheWritePerMtok": 3.75
                    }
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
    fn matches_vendor_prefixed_ids() {
        let spec = describe_with(&fixture(), "anthropic/claude-sonnet-4.5");
        assert_eq!(spec.source, "catalog");
        assert_eq!(spec.name, "Claude Sonnet");
        assert!(spec.tags.contains(&"coding".to_string()));
        assert_eq!(spec.context_window, Some(200_000));
        let pricing = spec.pricing.unwrap();
        assert_eq!(pricing.cache_read_per_mtok, Some(0.3));
        assert_eq!(pricing.cache_write_per_mtok, Some(3.75));
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
        spec.pricing = Some(Pricing {
            input_per_mtok: 1.0,
            output_per_mtok: 2.0,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        });
        let enriched = enrich_with(&fixture(), spec);
        assert_eq!(enriched.description, "Custom description");
        assert_eq!(enriched.pricing.unwrap().input_per_mtok, 1.0);
        assert_eq!(enriched.name, "Claude Sonnet");
    }
}
