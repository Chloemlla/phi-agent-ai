//! Durable provider profiles.
//!
//! The file lives next to the journal (`data_dir/providers.json`). It holds
//! endpoints, model mappings and *references* to credentials. List/read APIs
//! only ever expose a redacted hint; resolving a secret happens inside the
//! model gateway at call time.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::Utc;
use uuid::Uuid;

use phi_contracts::provider::{
    AuthMode, ModelSpec, Protocol, ProviderFile, ProviderInput, ProviderProfile, ProviderView,
    SecretRef,
};

use crate::model_catalog;
use phi_i18n as i18n;
use phi_kernel::fs_atomic::write_atomic;

#[derive(Clone, Debug)]
pub struct ResolvedProvider {
    pub profile: ProviderProfile,
    pub api_key: Option<String>,
}

pub struct ProviderStore {
    path: PathBuf,
    inner: RwLock<ProviderFile>,
}

impl ProviderStore {
    pub fn load(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let backup = backup_path(&path);
        // A corrupt primary file must fall back to the last good backup
        // instead of silently resetting the user's providers.
        let mut file = read_provider_file(&path)
            .or_else(|| read_provider_file(&backup))
            .unwrap_or_default();
        if file.version == 0 {
            file.version = 1;
        }
        // Repair a dangling active id instead of carrying it forward.
        if let Some(active) = file.active_id.clone() {
            let valid = file.providers.iter().any(|provider| {
                provider.id == active && provider.enabled && !provider.model.trim().is_empty()
            });
            if !valid {
                file.active_id = file
                    .providers
                    .iter()
                    .find(|provider| provider.enabled && !provider.model.trim().is_empty())
                    .map(|provider| provider.id.clone());
            }
        }
        if file.active_id.is_none() {
            file.active_id = file
                .providers
                .iter()
                .find(|provider| provider.enabled && !provider.model.trim().is_empty())
                .map(|provider| provider.id.clone());
        }
        Self {
            path,
            inner: RwLock::new(file),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn snapshot(&self) -> ProviderFile {
        self.inner.read().unwrap().clone()
    }

    pub fn views(&self) -> Vec<ProviderView> {
        let file = self.snapshot();
        let active = file.active_id.clone();
        let mut views: Vec<ProviderView> = file
            .providers
            .iter()
            .map(|profile| view_of(profile, active.as_deref() == Some(profile.id.as_str())))
            .collect();
        views.sort_by(|a, b| {
            a.sort_index
                .cmp(&b.sort_index)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        views
    }

    pub fn view(&self, id: &str) -> Option<ProviderView> {
        let file = self.snapshot();
        file.providers
            .iter()
            .find(|provider| provider.id == id)
            .map(|profile| view_of(profile, file.active_id.as_deref() == Some(id)))
    }

    pub fn active_id(&self) -> Option<String> {
        self.inner.read().unwrap().active_id.clone()
    }

    pub fn active(&self) -> Option<ProviderProfile> {
        let file = self.inner.read().unwrap();
        let id = file.active_id.as_deref()?;
        file.providers
            .iter()
            .find(|provider| provider.id == id)
            .cloned()
    }

    pub fn set_active(&self, id: &str) -> Result<(), String> {
        let mut file = self.inner.write().unwrap();
        let profile = file
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .ok_or_else(|| i18n::t_args("Unknown provider: {id}", &[("id", id.to_string())]))?;
        if !profile.enabled {
            return Err(i18n::t_args(
                "Provider {name} is disabled",
                &[("name", profile.name.clone())],
            ));
        }
        if profile.model.trim().is_empty() {
            return Err(i18n::t_args(
                "Provider {name} has no primary model yet",
                &[("name", profile.name.clone())],
            ));
        }
        file.active_id = Some(id.to_string());
        self.save(&file)?;
        Ok(())
    }

    /// Create or update a provider. `input.api_key == None` keeps the current
    /// credential; `Some("")` clears it.
    pub fn upsert(&self, input: ProviderInput) -> Result<ProviderView, String> {
        let mut file = self.inner.write().unwrap();
        let existing_index = match input.id.as_deref() {
            Some(id) => {
                let index = file
                    .providers
                    .iter()
                    .position(|provider| provider.id == id)
                    .ok_or_else(|| {
                        i18n::t_args("Unknown provider: {id}", &[("id", id.to_string())])
                    })?;
                Some(index)
            }
            None => None,
        };

        let now = Utc::now().to_rfc3339();
        let mut profile = match existing_index {
            Some(index) => file.providers[index].clone(),
            None => {
                let sort_index = file
                    .providers
                    .iter()
                    .map(|provider| provider.sort_index + 1)
                    .max()
                    .unwrap_or(0);
                ProviderProfile {
                    id: format!("prov_{}", Uuid::new_v4().simple()),
                    name: String::new(),
                    protocol: Protocol::AnthropicMessages,
                    base_url: String::new(),
                    api_key: None,
                    model: String::new(),
                    models: Vec::new(),
                    model_map: HashMap::new(),
                    auth: None,
                    headers: HashMap::new(),
                    max_output_tokens: None,
                    temperature: None,
                    enabled: true,
                    sort_index,
                    icon: None,
                    icon_color: None,
                    website_url: None,
                    notes: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                }
            }
        };

        let is_new = existing_index.is_none();
        let models_provided = input.models.is_some();
        let model_provided = input.model.is_some();
        if let Some(protocol) = input.protocol.as_deref() {
            profile.protocol = Protocol::parse(protocol)?;
            // Let the new protocol choose its default auth unless the caller
            // explicitly sets one below.
            if input.auth.is_none() {
                profile.auth = None;
            }
        }
        if let Some(name) = input.name {
            profile.name = name.trim().to_string();
        }
        if let Some(base_url) = input.base_url {
            profile.base_url = base_url.trim().trim_end_matches('/').to_string();
        }
        if let Some(model) = input.model {
            profile.model = model.trim().to_string();
        }
        if let Some(models) = input.models {
            profile.models = normalize_models(models);
        }
        if let Some(model_map) = input.model_map {
            profile.model_map = model_map;
        }
        if let Some(auth) = input.auth.as_deref() {
            profile.auth = Some(AuthMode::parse(auth)?);
        }
        if let Some(api_key) = input.api_key.as_deref() {
            let normalized = SecretRef::normalize(api_key);
            // A newly supplied key re-enables authentication even when an
            // earlier save explicitly stored "none"; otherwise the key would
            // be silently unused.
            if normalized.is_some() && input.auth.is_none() && profile.auth == Some(AuthMode::None)
            {
                profile.auth = None;
            }
            profile.api_key = normalized;
            if profile.api_key.is_none() && input.auth.is_none() {
                // Clearing the credential also drops the requirement for one;
                // an explicit auth mode in the same input still wins below.
                profile.auth = Some(AuthMode::None);
            }
        }
        if let Some(headers) = input.headers {
            validate_headers(&headers)?;
            profile.headers = headers;
        }
        if let Some(max_output_tokens) = input.max_output_tokens {
            if max_output_tokens == 0 {
                return Err(i18n::t("maxOutputTokens must be greater than 0"));
            }
            profile.max_output_tokens = Some(max_output_tokens);
        }
        if let Some(temperature) = input.temperature {
            if !(0.0..=2.0).contains(&temperature) {
                return Err(i18n::t("temperature must be between 0 and 2"));
            }
            profile.temperature = Some(temperature);
        }
        if input.clear_max_output_tokens {
            profile.max_output_tokens = None;
        }
        if input.clear_temperature {
            profile.temperature = None;
        }
        if let Some(enabled) = input.enabled {
            profile.enabled = enabled;
        }
        if let Some(sort_index) = input.sort_index {
            profile.sort_index = sort_index;
        }
        if input.icon.is_some() {
            profile.icon = input.icon;
        }
        if input.icon_color.is_some() {
            profile.icon_color = input.icon_color;
        }
        if input.website_url.is_some() {
            profile.website_url = input.website_url;
        }
        if input.notes.is_some() {
            profile.notes = input.notes;
        }

        // Keep the primary model and the model list consistent. When the
        // caller replaces the list without naming a primary, fall back to the
        // first model instead of resurrecting a removed one.
        if profile.model.is_empty()
            || (models_provided
                && !model_provided
                && !profile.models.is_empty()
                && !profile
                    .models
                    .iter()
                    .any(|model| model.id.eq_ignore_ascii_case(&profile.model)))
        {
            profile.model = profile
                .models
                .first()
                .map(|model| model.id.clone())
                .unwrap_or_default();
        }
        if !profile.model.is_empty()
            && !profile
                .models
                .iter()
                .any(|model| model.id.eq_ignore_ascii_case(&profile.model))
        {
            profile
                .models
                .push(model_catalog::enrich(ModelSpec::from_id(
                    profile.model.clone(),
                )));
        }
        profile.models = normalize_models(profile.models);

        if profile.name.is_empty() {
            return Err(i18n::t("Provider name cannot be empty"));
        }
        if profile.base_url.is_empty() {
            return Err(i18n::t("baseUrl cannot be empty"));
        }
        if !(profile.base_url.starts_with("http://") || profile.base_url.starts_with("https://")) {
            return Err(i18n::t("baseUrl must start with http:// or https://"));
        }
        // A draft provider may have no primary model yet; the settings page
        // can save it, fetch the provider's model list, and then pick one.
        // Using it for a call still fails honestly in `resolve_profile`.
        if profile.auth_mode() != AuthMode::None && profile.api_key.is_none() {
            return Err(i18n::t(
                "This auth mode requires an API key (an env:NAME reference works)",
            ));
        }

        profile.updated_at = now;
        match existing_index {
            Some(index) => file.providers[index] = profile.clone(),
            None => file.providers.push(profile.clone()),
        }
        if is_new && file.active_id.is_none() && profile.enabled && !profile.model.is_empty() {
            file.active_id = Some(profile.id.clone());
        }
        if file.active_id.is_none() && profile.enabled && !profile.model.is_empty() {
            file.active_id = Some(profile.id.clone());
        }
        // A saved draft (no model) must never keep the active slot when a
        // usable provider exists.
        let active_valid = file
            .active_id
            .as_deref()
            .and_then(|id| file.providers.iter().find(|provider| provider.id == id))
            .is_some_and(|provider| provider.enabled && !provider.model.trim().is_empty());
        if !active_valid {
            file.active_id = file
                .providers
                .iter()
                .find(|provider| provider.enabled && !provider.model.trim().is_empty())
                .map(|provider| provider.id.clone());
        }
        self.save(&file)?;
        let active = file.active_id.as_deref() == Some(profile.id.as_str());
        Ok(view_of(&profile, active))
    }

    pub fn remove(&self, id: &str) -> Result<bool, String> {
        let mut file = self.inner.write().unwrap();
        let before = file.providers.len();
        file.providers.retain(|provider| provider.id != id);
        if file.providers.len() == before {
            return Ok(false);
        }
        if file.active_id.as_deref() == Some(id) {
            file.active_id = file
                .providers
                .iter()
                .find(|provider| provider.enabled && !provider.model.trim().is_empty())
                .map(|provider| provider.id.clone());
        }
        self.save(&file)?;
        Ok(true)
    }

    /// Set (or clear) the credential of the active provider. If no provider
    /// exists, a default Anthropic profile is created from the environment.
    pub fn set_api_key(&self, key: &str) -> Result<ProviderView, String> {
        let active = self.active_id();
        if let Some(id) = active {
            return self.upsert(ProviderInput {
                id: Some(id),
                api_key: Some(key.to_string()),
                ..Default::default()
            });
        }
        if key.trim().is_empty() {
            return Err(i18n::t("No provider is available"));
        }
        let base_url = std::env::var("PHI_ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        let model = std::env::var("PHI_MODEL").unwrap_or_else(|_| "claude-sonnet-5".to_string());
        self.upsert(ProviderInput {
            name: Some("Anthropic".to_string()),
            protocol: Some("anthropic_messages".to_string()),
            base_url: Some(base_url),
            api_key: Some(key.to_string()),
            model: Some(model),
            auth: Some("x_api_key".to_string()),
            ..Default::default()
        })
    }

    /// First-run convenience: mirror the legacy environment variables into a
    /// provider profile. Only runs when no profile exists and a key is set,
    /// so it never overrides a user's explicit provider choice.
    pub fn seed_from_env(&self) -> Result<(), String> {
        {
            let file = self.inner.read().unwrap();
            if !file.providers.is_empty() {
                return Ok(());
            }
        }
        let (key_env, key_value) = match std::env::var("ANTHROPIC_API_KEY") {
            Ok(value) if !value.trim().is_empty() => ("ANTHROPIC_API_KEY", value),
            _ => match std::env::var("PHI_API_KEY") {
                Ok(value) if !value.trim().is_empty() => ("PHI_API_KEY", value),
                _ => return Ok(()),
            },
        };
        let _ = key_value;
        let base_url = std::env::var("PHI_ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        let model = std::env::var("PHI_MODEL").unwrap_or_else(|_| "claude-sonnet-5".to_string());
        self.upsert(ProviderInput {
            name: Some("Anthropic".to_string()),
            protocol: Some("anthropic_messages".to_string()),
            base_url: Some(base_url),
            api_key: Some(format!("env:{key_env}")),
            model: Some(model),
            auth: Some("x_api_key".to_string()),
            ..Default::default()
        })?;
        Ok(())
    }

    pub fn mark_models(&self, id: &str, models: Vec<String>) -> Result<(), String> {
        let mut file = self.inner.write().unwrap();
        let profile = file
            .providers
            .iter_mut()
            .find(|provider| provider.id == id)
            .ok_or_else(|| i18n::t_args("Unknown provider: {id}", &[("id", id.to_string())]))?;
        // Merge instead of replacing: fetched ids must not wipe descriptions
        // or limits the user already edited.
        for model_id in models {
            let model_id = model_id.trim();
            if model_id.is_empty() {
                continue;
            }
            if profile
                .models
                .iter()
                .any(|model| model.id.eq_ignore_ascii_case(model_id))
            {
                continue;
            }
            let mut spec = ModelSpec::from_id(model_id.to_string());
            spec.source = "fetched".to_string();
            profile.models.push(model_catalog::enrich(spec));
        }
        profile.models = normalize_models(profile.models.clone());
        if profile.model.is_empty() {
            if let Some(first) = profile.models.first() {
                profile.model = first.id.clone();
            }
        }
        profile.updated_at = Utc::now().to_rfc3339();
        self.save(&file)?;
        Ok(())
    }

    pub fn resolve_active(&self) -> Result<Option<ResolvedProvider>, String> {
        match self.active() {
            Some(profile) => self.resolve_profile(&profile).map(Some),
            None => Ok(None),
        }
    }

    pub fn resolve(&self, id: &str) -> Result<ResolvedProvider, String> {
        let file = self.snapshot();
        let profile = file
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .cloned()
            .ok_or_else(|| i18n::t_args("Unknown provider: {id}", &[("id", id.to_string())]))?;
        self.resolve_profile(&profile)
    }

    fn resolve_profile(&self, profile: &ProviderProfile) -> Result<ResolvedProvider, String> {
        self.resolve_profile_inner(profile, true)
    }

    /// Resolve endpoint and credentials without requiring a primary model.
    /// Listing/testing models must work on a freshly added, still-empty
    /// provider so the settings page can populate its model list.
    pub fn resolve_for_listing(&self, id: &str) -> Result<ResolvedProvider, String> {
        let file = self.snapshot();
        let profile = file
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .cloned()
            .ok_or_else(|| i18n::t_args("Unknown provider: {id}", &[("id", id.to_string())]))?;
        self.resolve_profile_inner(&profile, false)
    }

    pub fn resolve_active_for_listing(&self) -> Result<Option<ResolvedProvider>, String> {
        match self.active() {
            Some(profile) => self.resolve_profile_inner(&profile, false).map(Some),
            None => Ok(None),
        }
    }

    fn resolve_profile_inner(
        &self,
        profile: &ProviderProfile,
        require_model: bool,
    ) -> Result<ResolvedProvider, String> {
        if !profile.enabled {
            return Err(i18n::t_args(
                "Provider {name} is disabled",
                &[("name", profile.name.clone())],
            ));
        }
        if profile.base_url.trim().is_empty() {
            return Err(i18n::t_args(
                "Provider {name} is missing baseUrl",
                &[("name", profile.name.clone())],
            ));
        }
        if require_model && profile.model.trim().is_empty() {
            return Err(i18n::t_args(
                "Provider {name} is missing model",
                &[("name", profile.name.clone())],
            ));
        }
        let api_key = match profile.auth_mode() {
            AuthMode::None => None,
            _ => match &profile.api_key {
                Some(secret) => Some(secret.resolve().map_err(|error| {
                    i18n::t_args(
                        "Provider {name}: {error}",
                        &[("name", profile.name.clone()), ("error", error)],
                    )
                })?),
                None => {
                    return Err(i18n::t_args(
                        "Provider {name} has no API key",
                        &[("name", profile.name.clone())],
                    ));
                }
            },
        };
        Ok(ResolvedProvider {
            profile: profile.clone(),
            api_key,
        })
    }

    fn save(&self, file: &ProviderFile) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(file).map_err(|error| error.to_string())?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                i18n::t_args(
                    "Failed to create config directory: {error}",
                    &[("error", error.to_string())],
                )
            })?;
        }
        let backup = backup_path(&self.path);
        if self.path.exists() {
            let _ = fs::copy(&self.path, &backup);
        }
        write_atomic(&self.path, raw.as_bytes()).map_err(|error| {
            i18n::t_args(
                "Failed to replace provider config: {error}",
                &[("error", error)],
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

fn view_of(profile: &ProviderProfile, active: bool) -> ProviderView {
    ProviderView {
        id: profile.id.clone(),
        name: profile.name.clone(),
        protocol: profile.protocol.as_str().to_string(),
        base_url: profile.base_url.clone(),
        model: profile.model.clone(),
        models: profile.models.clone(),
        model_map: profile.model_map.clone(),
        headers: profile.headers.clone(),
        enabled: profile.enabled,
        active,
        has_key: profile.has_key(),
        key_hint: profile.api_key.as_ref().map(SecretRef::hint),
        auth: profile.auth_mode().as_str().to_string(),
        max_output_tokens: profile.max_output_tokens,
        temperature: profile.temperature,
        sort_index: profile.sort_index,
        icon: profile.icon.clone(),
        icon_color: profile.icon_color.clone(),
        website_url: profile.website_url.clone(),
        notes: profile.notes.clone(),
        created_at: profile.created_at.clone(),
        updated_at: profile.updated_at.clone(),
    }
}

fn normalize_models(models: Vec<ModelSpec>) -> Vec<ModelSpec> {
    let mut out: Vec<ModelSpec> = Vec::new();
    for model in models {
        let mut model = model;
        model.normalize();
        if model.id.is_empty() {
            continue;
        }
        if out
            .iter()
            .any(|existing| existing.id.eq_ignore_ascii_case(&model.id))
        {
            continue;
        }
        out.push(model_catalog::enrich(model));
    }
    out
}

fn validate_headers(headers: &HashMap<String, String>) -> Result<(), String> {
    for (key, value) in headers {
        let name = key.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(i18n::t("Custom header name cannot be empty"));
        }
        if name == "authorization" || name == "x-api-key" || name == "anthropic-version" {
            return Err(i18n::t_args(
                "Header {key} is managed by the gateway and cannot be overridden in custom headers",
                &[("key", key.clone())],
            ));
        }
        if key.contains('\r') || key.contains('\n') || value.contains('\r') || value.contains('\n')
        {
            return Err(i18n::t_args(
                "Header {key} contains an invalid newline",
                &[("key", key.clone())],
            ));
        }
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

fn read_provider_file(path: &Path) -> Option<ProviderFile> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<ProviderFile>(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> ProviderStore {
        let dir =
            std::env::temp_dir().join(format!("phi-providers-{name}-{}", uuid::Uuid::new_v4()));
        ProviderStore::load(dir.join("providers.json"))
    }

    fn input(name: &str) -> ProviderInput {
        ProviderInput {
            name: Some(name.to_string()),
            protocol: Some("openai_chat".to_string()),
            base_url: Some("https://example.com".to_string()),
            api_key: Some("env:PHI_TEST_KEY".to_string()),
            model: Some("test-model".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn add_activate_and_persist() {
        let store = store("persist");
        let view = store.upsert(input("One")).unwrap();
        assert!(view.active);
        assert_eq!(view.protocol, "openai_chat");
        assert_eq!(view.key_hint.as_deref(), Some("env:PHI_TEST_KEY"));
        assert!(!view.key_hint.as_deref().unwrap().contains("sk-"));

        let reloaded = ProviderStore::load(store.path().to_path_buf());
        assert_eq!(reloaded.views().len(), 1);
        assert_eq!(reloaded.active_id(), Some(view.id));
    }

    #[test]
    fn remove_active_moves_selection() {
        let store = store("remove");
        let first = store.upsert(input("One")).unwrap();
        let second = store.upsert(input("Two")).unwrap();
        assert!(first.active);
        assert!(store.remove(&first.id).unwrap());
        assert_eq!(store.active_id(), Some(second.id));
        assert!(!store.remove("missing").unwrap());
    }

    #[test]
    fn update_keeps_secret_when_omitted_and_clears_on_empty() {
        let store = store("secret");
        let view = store.upsert(input("One")).unwrap();
        let updated = store
            .upsert(ProviderInput {
                id: Some(view.id.clone()),
                name: Some("Renamed".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.key_hint.as_deref(), Some("env:PHI_TEST_KEY"));

        let cleared = store
            .upsert(ProviderInput {
                id: Some(view.id),
                api_key: Some(String::new()),
                ..Default::default()
            })
            .unwrap();
        assert!(cleared.key_hint.is_none());
    }

    #[test]
    fn rejects_auth_headers_in_custom_headers() {
        let store = store("headers");
        let mut input = input("Bad");
        input.headers = Some(HashMap::from([(
            "Authorization".to_string(),
            "Bearer x".to_string(),
        )]));
        assert!(store.upsert(input).is_err());
    }

    #[test]
    fn literal_secret_resolves_and_is_redacted() {
        let store = store("literal");
        let mut input = input("Literal");
        input.api_key = Some("sk-real-secret".to_string());
        let view = store.upsert(input).unwrap();
        assert_eq!(view.key_hint.as_deref(), Some("literal:***"));
        let resolved = store.resolve(&view.id).unwrap();
        assert_eq!(resolved.api_key.as_deref(), Some("sk-real-secret"));
    }

    #[test]
    fn corrupt_primary_falls_back_to_the_backup() {
        let store = store("backup");
        let view = store.upsert(input("One")).unwrap();
        let path = store.path().to_path_buf();
        fs::copy(&path, backup_path(&path)).unwrap();
        fs::write(&path, b"{not json").unwrap();

        let reloaded = ProviderStore::load(path);
        assert_eq!(reloaded.views().len(), 1);
        assert_eq!(reloaded.active_id(), Some(view.id));
    }

    #[test]
    fn a_primary_model_is_kept_in_the_model_list() {
        let store = store("primary-list");
        let view = store.upsert(input("One")).unwrap();
        assert!(
            view.models.iter().any(|model| model.id == "test-model"),
            "the primary model must appear in the list: {:?}",
            view.models
                .iter()
                .map(|model| &model.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn replacing_models_without_naming_a_primary_picks_the_first() {
        let store = store("primary-fallback");
        let view = store.upsert(input("One")).unwrap();
        let updated = store
            .upsert(ProviderInput {
                id: Some(view.id.clone()),
                models: Some(vec![
                    ModelSpec::from_id("alpha"),
                    ModelSpec::from_id("beta"),
                ]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(updated.model, "alpha");
    }

    #[test]
    fn fetched_models_merge_and_keep_existing_metadata() {
        let store = store("merge");
        let mut kept = ModelSpec::from_id("kept-model");
        kept.description = "do not lose".to_string();
        let view = store
            .upsert(ProviderInput {
                models: Some(vec![kept]),
                ..input("Merge")
            })
            .unwrap();

        store
            .mark_models(
                &view.id,
                vec!["new-model".to_string(), "kept-model".to_string()],
            )
            .unwrap();
        let after = store.view(&view.id).unwrap();
        let kept = after
            .models
            .iter()
            .find(|model| model.id == "kept-model")
            .unwrap();
        assert_eq!(kept.description, "do not lose");
        assert_eq!(
            after
                .models
                .iter()
                .filter(|model| model.id == "kept-model")
                .count(),
            1
        );
        let fetched = after
            .models
            .iter()
            .find(|model| model.id == "new-model")
            .unwrap();
        assert_eq!(fetched.source, "fetched");
    }

    #[test]
    fn drafts_can_be_listed_but_not_used_for_calls() {
        let store = store("draft");
        let view = store
            .upsert(ProviderInput {
                model: Some(String::new()),
                // A literal key resolves without relying on process env vars.
                api_key: Some("literal:test-key".to_string()),
                ..input("Draft")
            })
            .unwrap();
        assert!(view.model.is_empty());
        assert!(!view.active);
        assert!(store.resolve_for_listing(&view.id).is_ok());
        assert!(store.resolve(&view.id).is_err());
        assert!(store.set_active(&view.id).is_err());
    }

    #[test]
    fn draft_provider_never_holds_the_active_slot() {
        let store = store("draft-active");
        let draft = store
            .upsert(ProviderInput {
                model: Some(String::new()),
                api_key: Some("literal:test-key".to_string()),
                ..input("Draft")
            })
            .unwrap();
        assert!(!draft.active);

        let valid = store.upsert(input("Valid")).unwrap();
        assert!(valid.active, "a usable provider must take the active slot");

        let reloaded = ProviderStore::load(store.path().to_path_buf());
        assert_eq!(reloaded.active_id(), Some(valid.id));
    }
}
