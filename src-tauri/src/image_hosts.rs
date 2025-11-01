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

    // This is the primary location for plugins in production
    if let Ok(path) = app.path().resolve("plugins", BaseDirectory::Resource) {
        dirs.push(path);
    }

    // User-added plugins in the user's app config directory
    if let Ok(config_dir) = app.path().app_config_dir() {
        let user_plugin_dir = config_dir.join("plugins");
        dirs.push(user_plugin_dir);
    }

    // This is for development, to load plugins directly from the source directory
    #[cfg(debug_assertions)]
    {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(workspace_root) = manifest_dir.parent() {
            dirs.push(workspace_root.join("src").join("plugins"));
        } else {
            dirs.push(manifest_dir.join("src").join("plugins"));
        }
    }

    dirs
}

fn collect_plugins_from_dir(
    dir: &Path,
    collected: &mut BTreeMap<String, PathBuf>,
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
        collected.insert(id, path);
    }

    Ok(())
}

fn discover_plugins(app: &tauri::AppHandle) -> Result<Vec<PluginEntryPayload>, String> {
    // Android 平台：使用硬编码插件列表，因为无法通过 std::fs 遍历 APK assets
    #[cfg(target_os = "android")]
    let mut result: Vec<PluginEntryPayload> = {
        let mut plugins = Vec::new();

        // 硬编码的内置插件列表
        let builtin_plugins = ["freeimagehost", "sda1", "smms"];

        // 尝试从 Resource 目录获取基础路径
        if let Ok(resource_path) = app.path().resolve("plugins", BaseDirectory::Resource) {
            let base_path = resource_path
                .to_str()
                .unwrap_or("asset://localhost/plugins");
            debug!("Android: resolved plugins base path: {}", base_path);
            for plugin_id in &builtin_plugins {
                let script_path = format!("{}/{}.js", base_path, plugin_id);
                debug!(
                    "Android: adding builtin plugin {} with path: {}",
                    plugin_id, script_path
                );
                plugins.push(PluginEntryPayload {
                    id: plugin_id.to_string(),
                    script: script_path,
                });
            }
        }

        // 同时检查用户插件目录（这个可以正常访问）
        if let Ok(config_dir) = app.path().app_config_dir() {
            let user_plugin_dir = config_dir.join("plugins");
            let mut user_collected = BTreeMap::new();
            if let Err(err) = collect_plugins_from_dir(&user_plugin_dir, &mut user_collected) {
                debug!(
                    "collect_plugins_from_dir failed for user plugins {}: {}",
                    user_plugin_dir.display(),
                    err
                );
            }
            for (id, path) in user_collected {
                let script_path = path.to_str().unwrap_or("").to_string();
                plugins.push(PluginEntryPayload {
                    id,
                    script: script_path,
                });
            }
        }

        info!(
            "Android: discovered {} plugins (builtin + user)",
            plugins.len()
        );
        plugins
    };

    // 桌面平台：保持原有的动态发现逻辑
    #[cfg(not(target_os = "android"))]
    let mut result: Vec<PluginEntryPayload> = {
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

        collected
            .into_iter()
            .map(|(id, path)| {
                let mut script_path = path.to_str().unwrap_or("").to_string();
                if script_path.starts_with("\\\\?\\") {
                    script_path = script_path[4..].to_string();
                }
                PluginEntryPayload {
                    id,
                    script: script_path,
                }
            })
            .collect()
    };

    // 添加内置 S3 插件（所有平台）
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

/// 将指定的本地 JS 文件复制到资源插件目录并注册为新插件
#[tauri::command]
pub fn add_image_host_plugin(
    app: tauri::AppHandle,
    source: String,
) -> Result<PluginEntryPayload, String> {
    use std::fs;
    use std::path::PathBuf;
    // use tauri::path::BaseDirectory;

    // 验证源文件存在
    let src_path = PathBuf::from(&source);
    if !src_path.exists() {
        return Err(format!("源文件不存在: {}", source));
    }
    // 确保后缀合法
    let file_name = src_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("无法解析文件名: {}", source))?;
    if !(file_name.ends_with(".js") || file_name.ends_with(".mjs")) {
        return Err("仅支持 .js 或 .mjs 文件".into());
    }
    // 获取用户插件目录
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("获取用户配置目录失败: {e}"))?;
    let plugin_dir = config_dir.join("plugins");
    fs::create_dir_all(&plugin_dir).map_err(|e| format!("创建用户插件目录失败: {e}"))?;
    // 复制文件
    let dest_path = plugin_dir.join(file_name);
    fs::copy(&src_path, &dest_path).map_err(|e| format!("复制插件文件失败: {e}"))?;
    // 构建返回值
    let id = src_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
        .to_string();
    let script = dest_path.to_string_lossy().to_string();

    Ok(PluginEntryPayload { id, script })
}
