use std::fs;
use std::path::{Path, PathBuf};

use phi_contracts::auxiliary::ImageAsset;
use phi_i18n as i18n;
use sha2::{Digest, Sha256};

/// Content-addressed image asset store. Bytes live in `<id>.bin`; metadata
/// lives next to them in `<id>.json`. Assets are served by the shell from
/// local files only, never through a hot-linked remote URL.
pub struct FsAssetStore {
    root: PathBuf,
}

impl FsAssetStore {
    pub fn new(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn put(&self, bytes: &[u8], mut meta: ImageAsset) -> Result<ImageAsset, String> {
        let digest = Sha256::digest(bytes);
        let asset_id = hex::encode(digest);
        meta.asset_id = asset_id.clone();
        let bin = self.bin_path(&asset_id);
        let json = self.meta_path(&asset_id);
        if !bin.exists() {
            fs::write(&bin, bytes).map_err(|error| {
                i18n::t_args(
                    "Failed to write image asset: {error}",
                    &[("error", error.to_string())],
                )
            })?;
        }
        let raw = serde_json::to_string_pretty(&meta).map_err(|error| error.to_string())?;
        fs::write(&json, raw).map_err(|error| {
            i18n::t_args(
                "Failed to write image metadata: {error}",
                &[("error", error.to_string())],
            )
        })?;
        Ok(meta)
    }

    pub fn read(&self, asset_id: &str) -> Result<Vec<u8>, String> {
        let id = valid_id(asset_id)?;
        fs::read(self.bin_path(id)).map_err(|error| {
            i18n::t_args(
                "Image asset not found: {id} ({error})",
                &[("id", id.to_string()), ("error", error.to_string())],
            )
        })
    }

    pub fn meta(&self, asset_id: &str) -> Result<ImageAsset, String> {
        let id = valid_id(asset_id)?;
        let raw = fs::read_to_string(self.meta_path(id)).map_err(|error| {
            i18n::t_args(
                "Image asset not found: {id} ({error})",
                &[("id", id.to_string()), ("error", error.to_string())],
            )
        })?;
        serde_json::from_str(&raw).map_err(|error| {
            i18n::t_args(
                "Image metadata is unreadable: {error}",
                &[("error", error.to_string())],
            )
        })
    }

    pub fn list(&self, session_id: Option<&str>) -> Vec<ImageAsset> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut assets = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(asset) = serde_json::from_str::<ImageAsset>(&raw) else {
                continue;
            };
            if let Some(wanted) = session_id {
                if asset.session_id.as_deref() != Some(wanted) {
                    continue;
                }
            }
            assets.push(asset);
        }
        assets.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        assets
    }

    pub fn delete(&self, asset_id: &str) -> Result<bool, String> {
        let id = valid_id(asset_id)?;
        let bin = self.bin_path(id);
        let json = self.meta_path(id);
        let existed = bin.exists() || json.exists();
        if bin.exists() {
            let _ = fs::remove_file(&bin);
        }
        if json.exists() {
            let _ = fs::remove_file(&json);
        }
        Ok(existed)
    }

    fn bin_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.bin"))
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
}

fn valid_id(asset_id: &str) -> Result<&str, String> {
    let trimmed = asset_id.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(trimmed)
    } else {
        Err(i18n::t_args(
            "Invalid image asset id: {id}",
            &[("id", asset_id.to_string())],
        ))
    }
}
