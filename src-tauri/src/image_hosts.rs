use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;
use tauri::path::BaseDirectory;

const IMAGE_HOST_SETTINGS_FILE: &str = "image-hosts.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntryPayload {
    pub id: String,
    pub script: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ImageHostSettingsFile {
    #[serde(flatten)]
    plugins: HashMap<String, Value>,
}

fn ensure_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all {}: {e}", dir.display()))?;
    Ok(dir.join(IMAGE_HOST_SETTINGS_FILE))
}

fn read_settings(path: &Path) -> Result<ImageHostSettingsFile, String> {
    if !path.exists() {
        return Ok(ImageHostSettingsFile::default());
    }

    let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let parsed: ImageHostSettingsFile =
        serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(parsed)
}

fn write_settings(path: &Path, payload: &ImageHostSettingsFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(payload)
        .map_err(|e| format!("serialize image host settings: {e}"))?;
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

fn candidate_plugin_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(path) = app.path().resolve("plugins", BaseDirectory::Resource) {
        dirs.push(path);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(workspace_root) = manifest_dir.parent() {
        dirs.push(workspace_root.join("src").join("plugins"));
    } else {
        dirs.push(manifest_dir.join("src").join("plugins"));
    }

    dirs
}

fn collect_plugins_from_dir(
    dir: &Path,
    collected: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    if !dir.exists() {
        debug!("plugin directory not found, skipping: {}", dir.display());
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read_dir entry {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => {
                warn!(
                    "skip plugin file with invalid UTF-8 name: {}",
                    path.display()
                );
                continue;
            }
        };

        if !(file_name.ends_with(".js") || file_name.ends_with(".mjs")) {
            debug!("skip non-js plugin candidate: {}", file_name);
            continue;
        }

        let id = Path::new(file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file_name)
            .to_string();
        let script = format!("plugins/{file_name}");
        collected.insert(id, script);
    }

    Ok(())
}

fn discover_plugins(app: &tauri::AppHandle) -> Result<Vec<PluginEntryPayload>, String> {
    let mut collected = BTreeMap::new();
    let dirs = candidate_plugin_dirs(app);
    for dir in dirs {
        if let Err(err) = collect_plugins_from_dir(&dir, &mut collected) {
            warn!(
                "collect_plugins_from_dir failed for {}: {}",
                dir.display(),
                err
            );
        }
    }

    let mut result: Vec<PluginEntryPayload> = collected
        .into_iter()
        .map(|(id, script)| PluginEntryPayload { id, script })
        .collect();

    if !result.iter().any(|entry| entry.id == "s3") {
        result.push(PluginEntryPayload {
            id: "s3".to_string(),
            script: "__internal__/s3".to_string(),
        });
    }

    result.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(result)
}

#[tauri::command]
pub fn list_image_host_plugins(app: tauri::AppHandle) -> Result<Vec<PluginEntryPayload>, String> {
    let plugins = discover_plugins(&app)?;
    info!("list_image_host_plugins success: count={}", plugins.len());
    Ok(plugins)
}

#[tauri::command]
pub fn load_image_host_settings(
    app: tauri::AppHandle,
    plugin_id: String,
) -> Result<Option<Value>, String> {
    let path = ensure_config_path(&app)?;
    match read_settings(&path) {
        Ok(file) => {
            info!(
                "load_image_host_settings success: path={}, plugin_id={}",
                path.display(),
                plugin_id
            );
            Ok(file.plugins.get(&plugin_id).cloned())
        }
        Err(err) => {
            error!(
                "load_image_host_settings failed: path={}, plugin_id={}, error={}",
                path.display(),
                plugin_id,
                err
            );
            Err(err)
        }
    }
}

#[tauri::command]
pub fn save_image_host_settings(
    app: tauri::AppHandle,
    plugin_id: String,
    values: Value,
) -> Result<(), String> {
    let path = ensure_config_path(&app)?;
    let mut file = read_settings(&path)?;

    match values {
        Value::Object(_) => {
            file.plugins.insert(plugin_id.clone(), values);
        }
        Value::Null => {
            file.plugins.remove(&plugin_id);
        }
        other => {
            warn!(
                "save_image_host_settings received non-object value for plugin {}: {:?}",
                plugin_id, other
            );
            return Err("插件配置必须是对象".to_string());
        }
    }

    match write_settings(&path, &file) {
        Ok(()) => {
            info!(
                "save_image_host_settings success: path={}, plugin_id={}",
                path.display(),
                plugin_id
            );
            Ok(())
        }
        Err(err) => {
            error!(
                "save_image_host_settings failed: path={}, plugin_id={}, error={}",
                path.display(),
                plugin_id,
                err
            );
            Err(err)
        }
    }
}
