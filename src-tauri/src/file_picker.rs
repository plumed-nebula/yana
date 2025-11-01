use std::io::{Read, Write};
use tauri_plugin_android_fs::{AndroidFsExt, PrivateDir, PublicGeneralPurposeDir};

#[tauri::command]
pub async fn select_single_image(app: tauri::AppHandle) -> Result<String, String> {
    let files = select_images(app, false).await?;
    Ok(files.get(0).cloned().unwrap_or_default())
}

#[tauri::command]
pub async fn select_multiple_images(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    select_images(app, true).await
}

#[tauri::command]
pub async fn save_to_download_dir(
    app: tauri::AppHandle,
    source_path: String,
    file_name: String,
) -> Result<String, String> {
    let api = app.android_fs_async();

    // 读取源文件内容
    let source_bytes = std::fs::read(&source_path).map_err(|e| format!("读取源文件失败: {}", e))?;

    // 在 Download 目录创建新文件
    let file_uri = api
        .public_storage()
        .create_new_file(
            None, // 使用主存储卷
            PublicGeneralPurposeDir::Download,
            &file_name,
            None, // MIME 类型自动检测
        )
        .await
        .map_err(|e| format!("创建文件失败: {}", e))?;

    // 打开可写流并写入数据
    let mut stream = api
        .open_writable_stream(&file_uri)
        .await
        .map_err(|e| format!("打开写入流失败: {}", e))?;

    stream
        .write_all(&source_bytes)
        .map_err(|e| format!("写入数据失败: {}", e))?;

    stream.flush().map_err(|e| format!("刷新缓冲失败: {}", e))?;

    // 必须调用 reflect 来确保数据被写入
    let stream_sync = stream.into_sync();
    stream_sync
        .reflect()
        .map_err(|e| format!("反射数据失败: {}", e))?;

    // 通知媒体库扫描文件
    api.public_storage()
        .scan_file(&file_uri)
        .await
        .map_err(|e| format!("扫描文件失败: {}", e))?;

    Ok(format!("Download/{}", file_name))
}

async fn select_images(app: tauri::AppHandle, multiple: bool) -> Result<Vec<String>, String> {
    let api = app.android_fs_async();

    let picker = api.file_picker();
    let selected_files = if multiple {
        picker
            .pick_files(None, &["image/*"])
            .await
            .map_err(|e| e.to_string())?
    } else {
        picker
            .pick_file(None, &["image/*"])
            .await
            .map_err(|e| e.to_string())?
            .map_or(vec![], |f| vec![f])
    };

    if selected_files.is_empty() {
        return Ok(vec![]);
    }

    let temp_dir = api
        .private_storage()
        .resolve_path(PrivateDir::Cache)
        .await
        .map_err(|e| e.to_string())?;

    let mut result_paths = Vec::new();

    for uri in selected_files {
        let file_name = api.get_name(&uri).await.map_err(|e| e.to_string())?;
        let dest_path = temp_dir.join(&file_name);

        let mut source_file = api
            .open_file_readable(&uri)
            .await
            .map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        source_file
            .read_to_end(&mut buffer)
            .map_err(|e| e.to_string())?;

        std::fs::write(&dest_path, buffer).map_err(|e| e.to_string())?;

        result_paths.push(dest_path.to_string_lossy().to_string());
    }

    Ok(result_paths)
}
