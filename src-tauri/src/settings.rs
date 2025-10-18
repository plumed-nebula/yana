use std::fs;
use std::path::{Path, PathBuf};

use crate::process::{PngCompressionMode, PngOptimizationLevel};
use log::{error, info};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    pub quality: u8,
    pub convert_to_webp: bool,
    #[serde(default)]
    #[serde(alias = "pngMode")]
    pub png_compression_mode: PngCompressionMode,
    #[serde(default)]
    pub png_optimization: PngOptimizationLevel,
    #[serde(default)]
    pub enable_upload_compression: bool,
    #[serde(default = "default_max_concurrent_uploads")]
    pub max_concurrent_uploads: u8,
}

impl Default for SettingsPayload {
    fn default() -> Self {
        Self {
            quality: 80,
            convert_to_webp: false,
            png_compression_mode: PngCompressionMode::default(),
            png_optimization: PngOptimizationLevel::default(),
            enable_upload_compression: false,
            max_concurrent_uploads: default_max_concurrent_uploads(),
        }
    }
}

impl SettingsPayload {
    fn clamped(self) -> Self {
        Self {
            quality: self.quality.min(100),
            convert_to_webp: self.convert_to_webp,
            png_compression_mode: self.png_compression_mode,
            png_optimization: self.png_optimization,
            enable_upload_compression: self.enable_upload_compression,
            max_concurrent_uploads: self
                .max_concurrent_uploads
                .clamp(1, default_max_concurrent_uploads()),
        }
    }
}

const fn default_max_concurrent_uploads() -> u8 {
    5
}

fn ensure_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all {}: {e}", dir.display()))?;
    Ok(dir.join(SETTINGS_FILE))
}

fn read_payload(path: &Path) -> Result<SettingsPayload, String> {
    if !path.exists() {
        return Ok(SettingsPayload::default());
    }
    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let parsed: SettingsPayload =
        serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(parsed.clamped())
}

fn write_payload(path: &Path, payload: SettingsPayload) -> Result<(), String> {
    let sanitized = payload.clamped();
    let text =
        serde_json::to_string_pretty(&sanitized).map_err(|e| format!("serialize settings: {e}"))?;
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

#[tauri::command]
pub fn load_settings(app: tauri::AppHandle) -> Result<SettingsPayload, String> {
    let path = ensure_config_path(&app)?;
    match read_payload(&path) {
        Ok(payload) => {
            info!("load_settings success: path={}", path.display());
            Ok(payload)
        }
        Err(err) => {
            error!(
                "load_settings failed: path={}, error={}",
                path.display(),
                err
            );
            Err(err)
        }
    }
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: SettingsPayload) -> Result<(), String> {
    let path = ensure_config_path(&app)?;
    match write_payload(&path, settings) {
        Ok(()) => {
            info!("save_settings success: path={}", path.display());
            Ok(())
        }
        Err(err) => {
            error!(
                "save_settings failed: path={}, error={}",
                path.display(),
                err
            );
            Err(err)
        }
    }
}

#[tauri::command]
pub fn open_log_dir(app: tauri::AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("app_log_dir: {e}"))?;

    if let Err(err) = fs::create_dir_all(&path) {
        error!("open_log_dir create_dir_all failed: {}", err);
        return Err(format!("create_dir_all {}: {err}", path.display()));
    }

    let display_path = path.display().to_string();
    if let Err(err) = app
        .opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
    {
        error!("open_log_dir open_path failed: {}", err);
        return Err(format!("open_path {}: {err}", display_path));
    }

    info!("open_log_dir success: {}", display_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_serialization() {
        let settings = SettingsPayload {
            quality: 75,
            convert_to_webp: true,
            png_compression_mode: PngCompressionMode::Lossless,
            png_optimization: PngOptimizationLevel::Default,
            enable_upload_compression: true,
            max_concurrent_uploads: 3,
        };

        let json = serde_json::to_string_pretty(&settings).unwrap();
        println!("Serialized JSON:\n{}", json);

        // 验证字段名是驼峰式
        assert!(json.contains("\"quality\""));
        assert!(json.contains("\"convertToWebp\""));
        assert!(json.contains("\"forceAnimatedWebp\""));
        assert!(json.contains("\"pngCompressionMode\""));
        assert!(json.contains("\"pngOptimization\""));
        assert!(json.contains("\"enableUploadCompression\""));
        assert!(json.contains("\"maxConcurrentUploads\""));

        // 反序列化验证
        let deserialized: SettingsPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.quality, 75);
        assert_eq!(deserialized.convert_to_webp, true);
        assert_eq!(
            deserialized.png_compression_mode,
            PngCompressionMode::Lossless
        );
        assert_eq!(deserialized.png_optimization, PngOptimizationLevel::Default);
        assert_eq!(deserialized.enable_upload_compression, true);
        assert_eq!(deserialized.max_concurrent_uploads, 3);
    }
}
