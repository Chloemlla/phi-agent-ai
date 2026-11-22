//! Typed decision model (Jev-compatible Decisions API).
//!
//! The model receives a state and closed questions and returns calibrated
//! probabilities. It never produces prose and never executes tools; callers
//! branch on the answers with explicit thresholds and fail-safe paths.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::json_store::JsonStore;
use phi_contracts::SLOT_DECISION_EVALUATE;
use phi_contracts::auxiliary::{
    CompactionConfig, DecisionAnswer, DecisionConfigInput, DecisionPort, DecisionQuestion,
    DecisionRequest, DecisionResponse, DecisionStatus, TriageConfig,
};
use phi_contracts::provider::SecretRef;
use phi_contracts::util::truncate_chars;
use phi_domain::events::DomainEvent;
use phi_i18n as i18n;
use phi_kernel::event_bus::EventBus;
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::ports::{IdPort, LogPort};
use phi_kernel::registry::Contribution;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct DecisionConfig {
    enabled: bool,
    endpoint: String,
    api_key: Option<SecretRef>,
    model: String,
    timeout_ms: u64,
    triage_enabled: bool,
    triage_threshold: f32,
    triage_tools: Vec<String>,
    compaction_enabled: bool,
    compaction_threshold: f32,
    compaction_min_chars: usize,
    egress_granted: bool,
}

impl Default for DecisionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: String::new(),
            api_key: None,
            model: String::new(),
            timeout_ms: 8_000,
            triage_enabled: false,
            triage_threshold: 0.92,
            triage_tools: vec!["shell_exec".to_string(), "fs_write".to_string()],
            compaction_enabled: false,
            compaction_threshold: 0.8,
            compaction_min_chars: 8_000,
            egress_granted: false,
        }
    }
}

pub struct DecisionPlugin {
    config: JsonStore<DecisionConfig>,
    client: reqwest::Client,
    bus: Arc<EventBus>,
    ids: Arc<dyn IdPort>,
    log: Arc<dyn LogPort>,
}

impl DecisionPlugin {
    pub fn new(
        config_path: std::path::PathBuf,
        bus: Arc<EventBus>,
        ids: Arc<dyn IdPort>,
        log: Arc<dyn LogPort>,
    ) -> Self {
        Self {
            config: JsonStore::load(config_path),
            client: reqwest::Client::new(),
            bus,
            ids,
            log,
        }
    }

    fn reason(config: &DecisionConfig) -> Option<String> {
        if !config.enabled {
            return Some(i18n::t("The decision model is disabled"));
        }
        if config.endpoint.trim().is_empty() || config.model.trim().is_empty() {
            return Some(i18n::t("The decision model is not configured"));
        }
        if !config.egress_granted {
            return Some(i18n::t("Decision state egress is not granted"));
        }
        if config
            .api_key
            .as_ref()
            .is_none_or(|key| !key.is_configured())
        {
            return Some(i18n::t("The decision model API key is not configured"));
        }
        None
    }

    fn status_of(config: &DecisionConfig) -> DecisionStatus {
        let reason = Self::reason(config);
        DecisionStatus {
            configured: !config.endpoint.trim().is_empty() && !config.model.trim().is_empty(),
            enabled: config.enabled,
            ready: reason.is_none(),
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
            timeout_ms: config.timeout_ms,
            triage_enabled: config.triage_enabled,
            triage_threshold: config.triage_threshold,
            triage_tools: config.triage_tools.clone(),
            compaction_enabled: config.compaction_enabled,
            compaction_threshold: config.compaction_threshold,
            compaction_min_chars: config.compaction_min_chars,
            egress_granted: config.egress_granted,
            reason,
        }
    }

    fn apply_input(
        current: &DecisionConfig,
        input: DecisionConfigInput,
    ) -> Result<DecisionConfig, String> {
        let endpoint = input.endpoint.trim().trim_end_matches('/').to_string();
        if !endpoint.is_empty()
            && !endpoint.starts_with("http://")
            && !endpoint.starts_with("https://")
        {
            return Err(i18n::t(
                "The decision endpoint must start with http:// or https://",
            ));
        }
        if !(0.0..=1.0).contains(&input.triage_threshold)
            || !(0.0..=1.0).contains(&input.compaction_threshold)
        {
            return Err(i18n::t("Decision thresholds must be between 0 and 1"));
        }
        let api_key = match input.api_key {
            Some(value) => SecretRef::normalize(&value),
            None => current.api_key.clone(),
        };
        let mut tools: Vec<String> = input
            .triage_tools
            .into_iter()
            .map(|tool| tool.trim().to_string())
            .filter(|tool| !tool.is_empty())
            .collect();
        if tools.is_empty() {
            tools.push("fs_write".to_string());
        }
        Ok(DecisionConfig {
            enabled: input.enabled,
            endpoint,
            api_key,
            model: input.model.trim().to_string(),
            timeout_ms: input.timeout_ms.clamp(1_000, 30_000),
            triage_enabled: input.triage_enabled,
            triage_threshold: input.triage_threshold,
            triage_tools: tools,
            compaction_enabled: input.compaction_enabled,
            compaction_threshold: input.compaction_threshold,
            compaction_min_chars: input.compaction_min_chars.clamp(1_000, 100_000),
            egress_granted: input.egress_granted,
        })
    }

    fn parse_answer(question: &DecisionQuestion, value: &Value) -> Result<DecisionAnswer, String> {
        let clamp_unit = |number: f64| -> Result<f32, String> {
            if !(0.0..=1.0).contains(&number) || number.is_nan() {
                return Err(i18n::t_args(
                    "Decision probability is outside 0..1: {value}",
                    &[("value", number.to_string())],
                ));
            }
            Ok(number as f32)
        };
        match question {
            DecisionQuestion::Noul { .. } => {
                let probability = value
                    .get("probability")
                    .and_then(Value::as_f64)
                    .or_else(|| value.get("noul").and_then(Value::as_f64))
                    .or_else(|| value.as_f64())
                    .ok_or_else(|| {
                        i18n::t("The decision response is missing a noul probability")
                    })?;
                Ok(DecisionAnswer::Noul {
                    probability: clamp_unit(probability)?,
                })
            }
            DecisionQuestion::Choice { criteria, .. } => {
                let choice = value
                    .get("choice")
                    .and_then(Value::as_str)
                    .ok_or_else(|| i18n::t("The decision response is missing a choice"))?
                    .to_string();
                if !criteria.contains_key(&choice) {
                    return Err(i18n::t_args(
                        "The decision model chose an unknown option: {choice}",
                        &[("choice", choice)],
                    ));
                }
                let mut probabilities = BTreeMap::new();
                if let Some(map) = value.get("probabilities").and_then(Value::as_object) {
                    for (key, raw) in map {
                        if let Some(number) = raw.as_f64() {
                            probabilities.insert(key.clone(), clamp_unit(number)?);
                        }
                    }
                }
                let confidence = value
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .map(clamp_unit)
                    .transpose()?
                    .or_else(|| probabilities.get(&choice).copied())
                    .unwrap_or(0.0);
                Ok(DecisionAnswer::Choice {
                    choice,
                    probabilities,
                    confidence,
                })
            }
            DecisionQuestion::Score { .. } => {
                let score = value
                    .get("score")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| i18n::t("The decision response is missing a score"))?;
                let mut distribution = Vec::new();
                if let Some(values) = value.get("distribution").and_then(Value::as_array) {
                    for raw in values {
                        if let Some(number) = raw.as_f64() {
                            distribution.push(number as f32);
                        }
                    }
                }
                let confidence = value
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .map(clamp_unit)
                    .transpose()?
                    .unwrap_or(0.0);
                Ok(DecisionAnswer::Score {
                    score: score as f32,
                    distribution,
                    confidence,
                })
            }
        }
    }

    fn parse_response(
        call_id: String,
        request: &DecisionRequest,
        value: &Value,
    ) -> Result<DecisionResponse, String> {
        let answers_value = value
            .get("answers")
            .and_then(Value::as_object)
            .ok_or_else(|| i18n::t("The decision response has no answers object"))?;
        let mut answers = BTreeMap::new();
        for (id, question) in &request.questions {
            let raw = answers_value.get(id).ok_or_else(|| {
                i18n::t_args(
                    "The decision response is missing answer {id}",
                    &[("id", id.clone())],
                )
            })?;
            answers.insert(id.clone(), Self::parse_answer(question, raw)?);
        }
        let usage = value.get("usage").cloned().unwrap_or(Value::Null);
        let token = |key: &str| -> u64 {
            usage
                .get(key)
                .and_then(Value::as_u64)
                .or_else(|| value.get(key).and_then(Value::as_u64))
                .unwrap_or(0)
        };
        Ok(DecisionResponse {
            call_id,
            answers,
            input_tokens: token("input_tokens").max(token("prompt_tokens")),
            output_tokens: token("output_tokens").max(token("completion_tokens")),
        })
    }
}

#[async_trait]
impl DecisionPort for DecisionPlugin {
    fn status(&self) -> DecisionStatus {
        Self::status_of(&self.config.snapshot())
    }

    fn configure(&self, input: DecisionConfigInput) -> Result<DecisionStatus, String> {
        self.config.update(|current| {
            let next = Self::apply_input(current, input)?;
            *current = next;
            Ok::<(), String>(())
        })??;
        Ok(self.status())
    }

    fn triage(&self) -> TriageConfig {
        let config = self.config.snapshot();
        let usable = Self::reason(&config).is_none();
        TriageConfig {
            enabled: usable && config.triage_enabled,
            threshold: config.triage_threshold,
            tools: config.triage_tools,
        }
    }

    fn compaction(&self) -> CompactionConfig {
        let config = self.config.snapshot();
        let usable = Self::reason(&config).is_none();
        CompactionConfig {
            enabled: usable && config.compaction_enabled,
            threshold: config.compaction_threshold,
            min_chars: config.compaction_min_chars,
        }
    }

    async fn evaluate(
        &self,
        request: DecisionRequest,
        cancel: CancellationToken,
    ) -> Result<DecisionResponse, String> {
        let config = self.config.snapshot();
        if let Some(reason) = Self::reason(&config) {
            return Err(reason);
        }
        let key = config
            .api_key
            .as_ref()
            .ok_or_else(|| i18n::t("The decision model API key is not configured"))?
            .resolve()?;
        let call_id = self.ids.new_id("dec");
        let aggregate = request
            .session_id
            .clone()
            .unwrap_or_else(|| "node".to_string());
        let _ = self.bus.publish(
            &aggregate,
            "plugin-decision",
            &DomainEvent::DecisionRequested {
                call_id: call_id.clone(),
                point: request.point.clone(),
                model: config.model.clone(),
                state_chars: request.state.chars().count() as u64,
                session_id: request.session_id.clone(),
                run_id: request.run_id.clone(),
            },
        );
        let body = serde_json::json!({
            "model": config.model,
            "state": request.state,
            "questions": request.questions,
        });
        let started = Instant::now();
        let send = self
            .client
            .post(&config.endpoint)
            .bearer_auth(key)
            .json(&body)
            .timeout(Duration::from_millis(config.timeout_ms))
            .send();
        let response = tokio::select! {
            result = send => result,
            _ = cancel.cancelled() => return Err("cancelled".to_string()),
        };
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                let message = i18n::t_args(
                    "Decision request failed: {error}",
                    &[("error", error.to_string())],
                );
                let _ = self.bus.publish(
                    &aggregate,
                    "plugin-decision",
                    &DomainEvent::DecisionFailed {
                        call_id,
                        point: request.point.clone(),
                        error: message.clone(),
                    },
                );
                return Err(message);
            }
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let message = i18n::t_args(
                "Decision provider returned {status}: {body}",
                &[
                    ("status", status.as_u16().to_string()),
                    ("body", truncate_chars(&text, 1000)),
                ],
            );
            let _ = self.bus.publish(
                &aggregate,
                "plugin-decision",
                &DomainEvent::DecisionFailed {
                    call_id,
                    point: request.point.clone(),
                    error: message.clone(),
                },
            );
            return Err(message);
        }
        let value: Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(error) => {
                let message = i18n::t_args(
                    "Decision provider returned invalid JSON: {error}",
                    &[("error", error.to_string())],
                );
                let _ = self.bus.publish(
                    &aggregate,
                    "plugin-decision",
                    &DomainEvent::DecisionFailed {
                        call_id,
                        point: request.point.clone(),
                        error: message.clone(),
                    },
                );
                return Err(message);
            }
        };
        let parsed = match Self::parse_response(call_id.clone(), &request, &value) {
            Ok(parsed) => parsed,
            Err(error) => {
                let _ = self.bus.publish(
                    &aggregate,
                    "plugin-decision",
                    &DomainEvent::DecisionFailed {
                        call_id,
                        point: request.point.clone(),
                        error: error.clone(),
                    },
                );
                return Err(error);
            }
        };
        let _ = self.bus.publish(
            &aggregate,
            "plugin-decision",
            &DomainEvent::DecisionReturned {
                call_id,
                point: request.point.clone(),
                latency_ms: started.elapsed().as_millis() as u64,
                input_tokens: parsed.input_tokens,
                output_tokens: parsed.output_tokens,
            },
        );
        self.log.info(&format!(
            "decision {} answered in {:?}",
            request.point,
            started.elapsed()
        ));
        Ok(parsed)
    }
}

impl Plugin for DecisionPlugin {
    fn id(&self) -> &'static str {
        "plugin-decision"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_DECISION_EVALUATE.to_string(),
            id: "jev".to_string(),
        }])
    }
}
