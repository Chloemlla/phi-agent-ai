//! `image` command implementations.

use std::path::PathBuf;

use phi_contracts::auxiliary::{ImageConfigInput, ImageRequest};
use phi_i18n as i18n;

use crate::cli::GlobalOptions;

use super::support::{open_node, print_json};

pub(super) fn configure(global: &GlobalOptions, input: ImageConfigInput) -> Result<i32, String> {
    let node = open_node(global, true)?;
    print_json(&node.configure_image(input)?)?;
    Ok(0)
}

pub(super) fn show(global: &GlobalOptions) -> Result<i32, String> {
    let node = open_node(global, false)?;
    print_json(&node.image_status())?;
    Ok(0)
}

pub(super) async fn generate(
    global: &GlobalOptions,
    prompt: String,
    session_id: Option<String>,
    size: Option<String>,
    count: u32,
    out: Option<PathBuf>,
) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let response = node
        .generate_images(
            session_id.as_deref(),
            ImageRequest {
                prompt,
                size,
                count,
                reference_assets: Vec::new(),
            },
        )
        .await?;
    let mut written = Vec::new();
    if let Some(out) = out {
        for (index, asset) in response.assets.iter().enumerate() {
            let path = if response.assets.len() == 1 {
                out.clone()
            } else {
                let stem = out
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("image");
                let extension = out
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_else(|| match asset.mime.as_str() {
                        "image/jpeg" => "jpg",
                        "image/webp" => "webp",
                        "image/gif" => "gif",
                        "image/svg+xml" => "svg",
                        _ => "png",
                    });
                out.with_file_name(format!("{stem}-{}{extension}", index + 1))
            };
            let bytes = node.image_asset_bytes(&asset.asset_id)?;
            std::fs::write(&path, bytes).map_err(|error| {
                i18n::t_args(
                    "Failed to write image: {error}",
                    &[("error", error.to_string())],
                )
            })?;
            written.push(path.to_string_lossy().to_string());
        }
    }
    print_json(&serde_json::json!({
        "type": "image",
        "assets": response.assets,
        "written": written,
    }))?;
    Ok(0)
}

pub(super) fn list(global: &GlobalOptions, session_id: Option<String>) -> Result<i32, String> {
    let node = open_node(global, false)?;
    print_json(&serde_json::json!({
        "type": "images",
        "sessionId": session_id,
        "assets": node.image_assets(session_id.as_deref()),
    }))?;
    Ok(0)
}

pub(super) fn delete(global: &GlobalOptions, asset_id: &str) -> Result<i32, String> {
    let node = open_node(global, true)?;
    let deleted = node.delete_image_asset(asset_id)?;
    print_json(&serde_json::json!({
        "type": "image_deleted",
        "assetId": asset_id,
        "deleted": deleted,
    }))?;
    Ok(0)
}
