//! Starter model metadata and name-based auto-configuration.
//!
//! Provider profiles store model ids; this catalogue supplies a useful first
//! guess for the editable metadata (description, tags, limits, prices). Every
//! value is advisory and the settings UI can override it. Prices are
//! deliberately conservative upper bounds so a wrong guess never lets the
//! budget ledger under-count; verify them against the provider's price sheet.

use phi_contracts::provider::{ModelSpec, Pricing};

struct CatalogEntry {
    /// Lower-case substrings matched against the model id (specific first).
    patterns: &'static [&'static str],
    name: &'static str,
    description: &'static str,
    tags: &'static [&'static str],
    context_window: Option<u64>,
    max_output_tokens: Option<u32>,
    pricing: Option<Pricing>,
}

const ENTRIES: &[CatalogEntry] = &[
    CatalogEntry {
        patterns: &["claude-fable"],
        name: "Claude Fable 5",
        description: "Flagship model for long-horizon agent work and hard coding tasks.",
        tags: &["frontier", "reasoning", "coding", "tools", "long-context"],
        context_window: Some(200_000),
        max_output_tokens: Some(64_000),
        // Conservative placeholder until the provider price sheet is supplied.
        pricing: Some(Pricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
        }),
    },
    CatalogEntry {
        patterns: &["claude-opus"],
        name: "Claude Opus",
        description: "Deep reasoning model for complex coding and analysis.",
        tags: &["frontier", "reasoning", "coding", "tools"],
        context_window: Some(200_000),
        max_output_tokens: Some(32_000),
        pricing: Some(Pricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
        }),
    },
    CatalogEntry {
        patterns: &["claude-sonnet"],
        name: "Claude Sonnet",
        description: "Balanced model for everyday coding and agent tasks.",
        tags: &["reasoning", "coding", "tools", "balanced"],
        context_window: Some(200_000),
        max_output_tokens: Some(64_000),
        pricing: Some(Pricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        }),
    },
    CatalogEntry {
        patterns: &["claude-haiku"],
        name: "Claude Haiku",
        description: "Fast, low-cost model for short tasks and summaries.",
        tags: &["fast", "cheap", "tools"],
        context_window: Some(200_000),
        max_output_tokens: Some(32_000),
        pricing: Some(Pricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
        }),
    },
    CatalogEntry {
        patterns: &["gpt-5-nano"],
        name: "GPT-5 nano",
        description: "Smallest GPT-5 variant for cheap, high-volume calls.",
        tags: &["fast", "cheap", "tools"],
        context_window: Some(400_000),
        max_output_tokens: Some(128_000),
        pricing: Some(Pricing {
            input_per_mtok: 0.05,
            output_per_mtok: 0.40,
        }),
    },
    CatalogEntry {
        patterns: &["gpt-5-mini"],
        name: "GPT-5 mini",
        description: "Mid-size GPT-5 variant for routine agent work.",
        tags: &["fast", "balanced", "tools"],
        context_window: Some(400_000),
        max_output_tokens: Some(128_000),
        pricing: Some(Pricing {
            input_per_mtok: 0.25,
            output_per_mtok: 2.0,
        }),
    },
    CatalogEntry {
        patterns: &["gpt-5"],
        name: "GPT-5",
        description: "OpenAI flagship model for general reasoning and tool use.",
        tags: &["frontier", "reasoning", "coding", "tools"],
        context_window: Some(400_000),
        max_output_tokens: Some(128_000),
        pricing: Some(Pricing {
            input_per_mtok: 1.25,
            output_per_mtok: 10.0,
        }),
    },
    CatalogEntry {
        patterns: &["gpt-4.1-nano", "gpt-4-1-nano"],
        name: "GPT-4.1 nano",
        description: "Fastest GPT-4.1 variant for classification and extraction.",
        tags: &["fast", "cheap", "tools"],
        context_window: Some(1_047_576),
        max_output_tokens: Some(32_768),
        pricing: Some(Pricing {
            input_per_mtok: 0.10,
            output_per_mtok: 0.40,
        }),
    },
    CatalogEntry {
        patterns: &["gpt-4.1-mini", "gpt-4-1-mini"],
        name: "GPT-4.1 mini",
        description: "Cost-efficient GPT-4.1 variant with a large context window.",
        tags: &["fast", "balanced", "tools"],
        context_window: Some(1_047_576),
        max_output_tokens: Some(32_768),
        pricing: Some(Pricing {
            input_per_mtok: 0.40,
            output_per_mtok: 1.60,
        }),
    },
    CatalogEntry {
        patterns: &["gpt-4.1", "gpt-4-1"],
        name: "GPT-4.1",
        description: "General-purpose OpenAI model with a very large context window.",
        tags: &["coding", "tools", "long-context"],
        context_window: Some(1_047_576),
        max_output_tokens: Some(32_768),
        pricing: Some(Pricing {
            input_per_mtok: 2.0,
            output_per_mtok: 8.0,
        }),
    },
    CatalogEntry {
        patterns: &["o4-mini"],
        name: "o4-mini",
        description: "Reasoning model tuned for fast, low-cost problem solving.",
        tags: &["reasoning", "fast", "tools"],
        context_window: Some(200_000),
        max_output_tokens: Some(100_000),
        pricing: Some(Pricing {
            input_per_mtok: 1.10,
            output_per_mtok: 4.40,
        }),
    },
    CatalogEntry {
        patterns: &["o3"],
        name: "o3",
        description: "Reasoning model for difficult multi-step problems.",
        tags: &["reasoning", "tools"],
        context_window: Some(200_000),
        max_output_tokens: Some(100_000),
        pricing: Some(Pricing {
            input_per_mtok: 2.0,
            output_per_mtok: 8.0,
        }),
    },
    CatalogEntry {
        patterns: &["gemini-2.5-flash"],
        name: "Gemini 2.5 Flash",
        description: "Fast multimodal Gemini model with a very large context.",
        tags: &["fast", "cheap", "vision", "long-context", "tools"],
        context_window: Some(1_048_576),
        max_output_tokens: Some(65_536),
        pricing: Some(Pricing {
            input_per_mtok: 0.30,
            output_per_mtok: 2.50,
        }),
    },
    CatalogEntry {
        patterns: &["gemini-2.5-pro"],
        name: "Gemini 2.5 Pro",
        description: "Gemini flagship for deep reasoning and long documents.",
        tags: &["frontier", "reasoning", "vision", "long-context", "tools"],
        context_window: Some(1_048_576),
        max_output_tokens: Some(65_536),
        pricing: Some(Pricing {
            input_per_mtok: 1.25,
            output_per_mtok: 10.0,
        }),
    },
    CatalogEntry {
        patterns: &["deepseek-reasoner"],
        name: "DeepSeek Reasoner",
        description: "Open-weights reasoning model with visible chain of thought.",
        tags: &["reasoning", "open-weights", "cheap"],
        context_window: Some(128_000),
        max_output_tokens: Some(32_768),
        pricing: Some(Pricing {
            input_per_mtok: 0.55,
            output_per_mtok: 2.19,
        }),
    },
    CatalogEntry {
        patterns: &["deepseek-chat"],
        name: "DeepSeek Chat",
        description: "General-purpose DeepSeek model for chat and coding.",
        tags: &["coding", "open-weights", "cheap"],
        context_window: Some(128_000),
        max_output_tokens: Some(8_192),
        pricing: Some(Pricing {
            input_per_mtok: 0.27,
            output_per_mtok: 1.10,
        }),
    },
    CatalogEntry {
        patterns: &["qwen3", "qwen-3", "qwen-max", "qwen-plus"],
        name: "Qwen",
        description: "Alibaba Qwen model, strong on coding and Chinese content.",
        tags: &["coding", "multilingual", "open-weights", "tools"],
        context_window: Some(131_072),
        max_output_tokens: Some(32_768),
        pricing: None,
    },
    CatalogEntry {
        patterns: &["kimi-k2", "kimi-k2.5"],
        name: "Kimi K2",
        description: "Moonshot agentic model with a large context window.",
        tags: &["coding", "tools", "long-context", "open-weights"],
        context_window: Some(262_144),
        max_output_tokens: Some(32_768),
        pricing: None,
    },
    CatalogEntry {
        patterns: &["glm-4.6", "glm-4-6"],
        name: "GLM 4.6",
        description: "Zhipu model for coding and tool use.",
        tags: &["coding", "tools", "open-weights"],
        context_window: Some(204_800),
        max_output_tokens: Some(131_072),
        pricing: None,
    },
    CatalogEntry {
        patterns: &["llama", "mistral", "qwen2", "phi-", "gemma"],
        name: "Local model",
        description: "Local or self-hosted model served through an OpenAI-compatible API.",
        tags: &["local", "open-weights"],
        context_window: None,
        max_output_tokens: None,
        pricing: None,
    },
];

fn normalized(id: &str) -> String {
    let lower = id.trim().to_ascii_lowercase();
    // OpenRouter-style ids prefix the vendor; match on the model part.
    lower.rsplit('/').next().unwrap_or(&lower).to_string()
}

fn match_entry(id: &str) -> Option<&'static CatalogEntry> {
    let id = normalized(id);
    ENTRIES
        .iter()
        .find(|entry| entry.patterns.iter().any(|pattern| id.contains(pattern)))
}

/// Best-effort metadata for a model id. Never fails: an unknown id comes back
/// as a manual spec with only the id filled in.
pub fn describe(id: &str) -> ModelSpec {
    let id = id.trim();
    let Some(entry) = match_entry(id) else {
        return ModelSpec::from_id(id);
    };
    ModelSpec {
        id: id.to_string(),
        name: entry.name.to_string(),
        description: entry.description.to_string(),
        tags: entry.tags.iter().map(|tag| tag.to_string()).collect(),
        context_window: entry.context_window,
        max_output_tokens: entry.max_output_tokens,
        pricing: entry.pricing,
        temperature: None,
        enabled: true,
        source: "catalog".to_string(),
    }
}

pub fn describe_many(ids: &[String]) -> Vec<ModelSpec> {
    ids.iter().map(|id| describe(id)).collect()
}

/// Fill only the metadata the caller left empty. Explicit values always win,
/// so a hand-edited price or description survives a re-save. A spec marked
/// `manual` keeps its numeric fields untouched: clearing a price or limit is
/// how the user asks to inherit the provider default.
pub fn enrich(mut spec: ModelSpec) -> ModelSpec {
    spec.normalize();
    if spec.id.is_empty() {
        return spec;
    }
    let suggested = describe(&spec.id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_vendor_prefixed_ids() {
        let spec = describe("anthropic/claude-sonnet-4.5");
        assert_eq!(spec.source, "catalog");
        assert_eq!(spec.name, "Claude Sonnet");
        assert!(spec.tags.contains(&"coding".to_string()));
    }

    #[test]
    fn specific_variants_win_over_generic_ones() {
        assert_eq!(describe("gpt-5-mini").name, "GPT-5 mini");
        assert_eq!(describe("gpt-5").name, "GPT-5");
        assert_eq!(describe("claude-haiku-4-5").name, "Claude Haiku");
    }

    #[test]
    fn unknown_models_stay_manual() {
        let spec = describe("my-private-model");
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
        });
        let enriched = enrich(spec);
        assert_eq!(enriched.description, "Custom description");
        assert_eq!(enriched.pricing.unwrap().input_per_mtok, 1.0);
        assert_eq!(enriched.name, "Claude Sonnet");
    }
}
