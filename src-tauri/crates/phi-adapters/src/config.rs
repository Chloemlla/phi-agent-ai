use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use phi_i18n as i18n;
use phi_kernel::ports::LogPort;
use serde::{Deserialize, Serialize};

use phi_kernel::fs_atomic::write_atomic;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub node_id: Option<String>,
    pub workspace: Option<String>,
    /// BCP 47 tag such as `en` or `zh-CN`. Missing means "detect".
    pub language: Option<String>,
}

pub struct ConfigStore {
    path: PathBuf,
    inner: Mutex<AppConfig>,
}

impl ConfigStore {
    pub fn load(path: PathBuf, log: Arc<dyn LogPort>) -> Self {
        let inner = match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
                Ok(config) => config,
                Err(error) => {
                    log.warn(&format!(
                        "settings: {} is unreadable ({error}); using defaults",
                        path.display()
                    ));
                    AppConfig::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
            Err(error) => {
                log.warn(&format!(
                    "settings: cannot read {} ({error}); using defaults",
                    path.display()
                ));
                AppConfig::default()
            }
        };
        Self {
            path,
            inner: Mutex::new(inner),
        }
    }

    pub fn snapshot(&self) -> AppConfig {
        self.inner.lock().unwrap().clone()
    }

    /// Mutate and persist. Returns an error when the new state cannot be made
    /// durable, so callers never report success for a lost setting. The
    /// in-memory snapshot only advances after the write succeeds.
    pub fn update<T>(&self, mutate: impl FnOnce(&mut AppConfig) -> T) -> Result<T, String> {
        let mut guard = self.inner.lock().unwrap();
        let mut next = guard.clone();
        let result = mutate(&mut next);
        let raw = serde_json::to_string_pretty(&next).map_err(|error| {
            i18n::t_args(
                "Failed to serialize settings: {error}",
                &[("error", error.to_string())],
            )
        })?;
        write_atomic(&self.path, raw.as_bytes()).map_err(|error| {
            i18n::t_args("Failed to save settings: {error}", &[("error", error)])
        })?;
        *guard = next;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::StderrLog;

    fn log() -> Arc<dyn LogPort> {
        Arc::new(StderrLog)
    }

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("phi-config-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn update_persists_new_value_and_snapshot_reflects_it() {
        let dir = temp_dir("success");
        let path = dir.join("config.json");
        let store = ConfigStore::load(path.clone(), log());
        assert_eq!(store.snapshot().language, None);

        let returned = store
            .update(|config| {
                config.language = Some("zh-CN".to_string());
                config.workspace = Some("workspace".to_string());
                config.language.clone()
            })
            .unwrap();

        assert_eq!(returned.as_deref(), Some("zh-CN"));
        let snapshot = store.snapshot();
        assert_eq!(snapshot.language.as_deref(), Some("zh-CN"));
        assert_eq!(snapshot.workspace.as_deref(), Some("workspace"));

        let raw = fs::read_to_string(&path).unwrap();
        let persisted: AppConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(persisted.language.as_deref(), Some("zh-CN"));
        assert_eq!(persisted.workspace.as_deref(), Some("workspace"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn failed_write_leaves_snapshot_unchanged() {
        let dir = temp_dir("failure");
        // A regular file cannot become the parent directory, so `write_atomic`
        // fails while creating the parent of the store path.
        let blocker = dir.join("not-a-directory");
        fs::write(&blocker, b"blocker").unwrap();
        let store = ConfigStore::load(blocker.join("config.json"), log());
        let before = store.snapshot();
        assert_eq!(before.language, None);

        let error = store
            .update(|config| {
                config.language = Some("zh-CN".to_string());
                config.workspace = Some("workspace".to_string());
            })
            .unwrap_err();

        assert!(!error.is_empty());
        let after = store.snapshot();
        assert_eq!(after.language, before.language);
        assert_eq!(after.workspace, before.workspace);
        assert_eq!(after.node_id, before.node_id);

        fs::remove_dir_all(&dir).unwrap();
    }
}
