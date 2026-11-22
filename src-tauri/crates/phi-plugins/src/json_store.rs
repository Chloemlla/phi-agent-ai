use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use phi_i18n as i18n;
use phi_kernel::fs_atomic::write_atomic;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Small JSON-backed settings store for optional surfaces. Mutations are
/// written before they become visible in memory, so a failed write never
/// reports success.
pub struct JsonStore<T> {
    path: PathBuf,
    inner: Mutex<T>,
}

impl<T> JsonStore<T>
where
    T: Clone + Default + Serialize + DeserializeOwned,
{
    pub fn load(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let inner = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<T>(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(inner),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn snapshot(&self) -> T {
        self.inner.lock().unwrap().clone()
    }

    pub fn update<R>(&self, mutate: impl FnOnce(&mut T) -> R) -> Result<R, String> {
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
