/*
缩略图模块职责：
1) 接收前端传入的图片 URL 列表；
2) 异步下载图片到系统临时目录（I/O 密集，使用异步）；
3) 检查应用数据目录下的 cache 文件夹中是否存在该图片的缓存（使用 hash 值）；
4) 如果缓存不存在，则进行压缩（CPU 密集操作）；
5) 如果缓存存在，直接返回缓存地址；
6) 返回缩略图文件的本地路径数组。

设计说明：
- 临时文件存储在系统临时目录 (std::env::temp_dir()/com.yana.dev)
- 网络下载部分使用异步（I/O 密集，使用 futures::join_all 并发）
- 图片压缩部分在异步上下文中直接执行（同步 CPU 密集）
- 参考 process.rs 的架构模式
*/

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use log::{debug, error, info};
use tauri::{AppHandle, Manager};

const CACHE_DIR_NAME: &str = "cache";
const THUMBNAIL_WIDTH: u32 = 320;
const THUMBNAIL_HEIGHT: u32 = 225; // 320 * 0.70 ≈ 224，与前端 70% padding-top 对应

/// 获取应用临时目录（系统 temp 下的 com.yana.dev）
fn app_temp_dir() -> Result<PathBuf, String> {
    let mut dir = std::env::temp_dir();
    dir.push("com.yana.dev");
    Ok(dir)
}

/// 确保应用临时目录存在
fn ensure_app_temp_dir() -> Result<PathBuf, String> {
    let dir = app_temp_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create app temp dir {}: {}", dir.display(), e))?;
    Ok(dir)
}

/// 获取应用数据目录下的缓存文件夹路径
fn get_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Failed to resolve app data dir: {err}"))?;

    let cache_dir = app_data_dir.join(CACHE_DIR_NAME);

    // 创建缓存目录
    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache dir {}: {}", cache_dir.display(), e))?;

    Ok(cache_dir)
}

/// 计算 URL 的 hash（用作缓存文件名）
fn compute_url_hash(url: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash_value = hasher.finish();
    format!("{:016x}", hash_value)
}

/// 从 URL 提取文件扩展名
fn extract_file_extension(url: &str) -> String {
    url.split('?')
        .next()
        .and_then(|s| s.split('/').last())
        .and_then(|filename| {
            if let Some(pos) = filename.rfind('.') {
                Some(filename[pos..].to_lowercase())
            } else {
                None
            }
        })
        .unwrap_or_else(|| ".jpg".to_string())
}

/// 生成缓存文件路径（只用 hash，不含原始文件名）
/// 例如：hash_value.webp
fn generate_cache_path(cache_dir: &PathBuf, url: &str) -> PathBuf {
    let hash = compute_url_hash(url);
    cache_dir.join(format!("{}.webp", hash))
}

/// 下载图片到指定路径（异步 I/O）
async fn download_image(url: &str, dest_path: &PathBuf) -> Result<u64, String> {
    debug!("Downloading image from URL: {}", url);

    let client = reqwest::Client::new();
    let mut response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Failed to download image from {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP error {} when downloading from {}",
            response.status(),
            url
        ));
    }

    // 获取 Content-Length 以防止下载过大的文件
    let max_size = 50 * 1024 * 1024; // 50MB 限制
    if let Some(content_length) = response.content_length() {
        if content_length > max_size {
            return Err(format!(
                "Image too large ({} bytes) from {} (max: {} bytes)",
                content_length, url, max_size
            ));
        }
    }

    // 使用流式读取，处理 chunked encoding 等特殊情况
    let mut file = std::fs::File::create(dest_path)
        .map_err(|e| format!("Failed to create file {}: {}", dest_path.display(), e))?;

    let mut downloaded_size = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Failed to read response chunk: {}", e))?
    {
        // 防止下载超过 50MB
        downloaded_size += chunk.len() as u64;
        if downloaded_size > max_size {
            return Err(format!(
                "Downloaded data exceeds maximum size ({} bytes) from {}",
                max_size, url
            ));
        }

        file.write_all(&chunk)
            .map_err(|e| format!("Failed to write to file: {}", e))?;
    }

    // 检查是否为空
    if downloaded_size == 0 {
        return Err(format!("Downloaded image is empty from {}", url));
    }

    debug!(
        "Downloaded image to: {}, size: {} bytes",
        dest_path.display(),
        downloaded_size
    );
    Ok(downloaded_size)
}

/// 压缩图片到缩略图尺寸（同步 CPU 密集操作）
fn compress_to_thumbnail(input_path: &PathBuf, output_path: &PathBuf) -> Result<u64, String> {
    debug!(
        "Compressing image: {} -> {}",
        input_path.display(),
        output_path.display()
    );

    // 读取图片
    let img = image::open(input_path)
        .map_err(|e| format!("Failed to open image {}: {}", input_path.display(), e))?;

    // 按照缩略图尺寸调整大小，使用 Lanczos3 过滤（高质量）
    let thumbnail = img.thumbnail(THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT);

    // 转换为 WebP 格式以获得更好的压缩比
    thumbnail
        .save_with_format(output_path, image::ImageFormat::WebP)
        .map_err(|e| {
            format!(
                "Failed to save thumbnail to {}: {}",
                output_path.display(),
                e
            )
        })?;

    // 获取输出文件大小
    let output_size = fs::metadata(output_path)
        .map_err(|e| format!("Failed to get output file metadata: {}", e))?
        .len();

    debug!(
        "Thumbnail created successfully: {}, size: {} bytes",
        output_path.display(),
        output_size
    );
    Ok(output_size)
}

/// 处理单个 URL：下载、压缩或返回缓存
/// 这是一个异步函数，压缩部分直接在异步任务中执行
async fn process_single_thumbnail(
    url: String,
    cache_dir: PathBuf,
    temp_dir: PathBuf,
) -> Result<String, String> {
    debug!("Processing thumbnail for URL: {}", url);

    // 生成缓存文件路径
    let cache_path = generate_cache_path(&cache_dir, &url);

    // 检查缓存是否存在
    if cache_path.exists() {
        let cache_size = fs::metadata(&cache_path).ok().map(|m| m.len()).unwrap_or(0);
        debug!(
            "Thumbnail cache exists: {}, size: {} bytes",
            cache_path.to_string_lossy(),
            cache_size
        );
        return Ok(cache_path.to_string_lossy().to_string());
    }

    // 创建临时文件用于下载（使用 UUID + 原始扩展名）
    let ext = extract_file_extension(&url);
    let temp_path = temp_dir.join(format!("thumb_{}{}", uuid::Uuid::new_v4(), ext));

    // 下载图片
    let download_size = download_image(&url, &temp_path).await?;

    // 压缩为缩略图
    let thumbnail_size = compress_to_thumbnail(&temp_path, &cache_path)?;

    // 清理临时文件
    if let Err(e) = fs::remove_file(&temp_path) {
        error!(
            "Failed to remove temporary file {}: {}",
            temp_path.display(),
            e
        );
    }

    info!(
        "Thumbnail generated: {} (download: {} bytes, thumbnail: {} bytes)",
        cache_path.to_string_lossy(),
        download_size,
        thumbnail_size
    );

    Ok(cache_path.to_string_lossy().to_string())
}

/// 生成一组图片的缩略图
///
/// # 设计说明
/// - 下载部分使用异步并发（futures::join_all）处理 I/O 密集操作
/// - 压缩部分在异步上下文中同步执行（简化设计）
/// - 如果后续需要更高性能，可改为用 tokio::spawn_blocking + rayon 处理压缩
///
/// # 参数
/// - `urls`: 图片 URL 列表
///
/// # 返回
/// 成功返回对应的缩略图本地路径列表（顺序与输入一致）
/// 失败时返回错误信息
#[tauri::command]
pub async fn generate_thumbnails(app: AppHandle, urls: Vec<String>) -> Result<Vec<String>, String> {
    info!(
        "generate_thumbnails start: count={}, urls={:?}",
        urls.len(),
        urls
    );

    let cache_dir = get_cache_dir(&app)?;
    let temp_dir = ensure_app_temp_dir()?;

    // 并发处理所有 URL 的下载和压缩
    // 使用 futures 并发处理（保持顺序）
    let mut tasks = Vec::new();
    for url in urls {
        let cache_dir_clone = cache_dir.clone();
        let temp_dir_clone = temp_dir.clone();
        tasks.push(process_single_thumbnail(
            url,
            cache_dir_clone,
            temp_dir_clone,
        ));
    }

    // 并发执行所有任务
    let results = futures::future::join_all(tasks).await;
    let mut output = Vec::new();
    let mut failed_count = 0;

    for (idx, result) in results.into_iter().enumerate() {
        match result {
            Ok(path) => {
                output.push(path);
            }
            Err(e) => {
                failed_count += 1;
                error!("Failed to generate thumbnail for URL index {}: {}", idx, e);
                // 跳过失败的图片，继续处理其他图片
                // 这样可以保证即使某些图片失败，其他图片仍能处理
            }
        }
    }

    // 如果全部失败，返回错误；否则返回成功的缩略图路径
    if failed_count > 0 && output.is_empty() {
        return Err(format!(
            "Failed to generate all {} thumbnails",
            failed_count
        ));
    }

    if failed_count > 0 {
        info!(
            "generate_thumbnails done: count={}, failed={}",
            output.len(),
            failed_count
        );
    } else {
        info!("generate_thumbnails done: count={}", output.len());
    }
    Ok(output)
}

/// 获取单个图片的缩略图本地路径（如果存在）
#[tauri::command]
pub fn get_thumbnail_path(app: AppHandle, url: String) -> Result<Option<String>, String> {
    let cache_dir = get_cache_dir(&app)?;
    let cache_path = generate_cache_path(&cache_dir, &url);

    if cache_path.exists() {
        // 返回文件路径字符串（前端将使用 file:// 协议）
        let path_str = cache_path
            .to_str()
            .ok_or("Failed to convert path to string")?
            .to_string();
        Ok(Some(path_str))
    } else {
        Ok(None)
    }
}

/// 清理所有缓存的缩略图
#[tauri::command]
pub fn clear_thumbnail_cache(app: AppHandle) -> Result<(), String> {
    info!("clear_thumbnail_cache start");

    let cache_dir = get_cache_dir(&app)?;

    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir).map_err(|e| format!("Failed to remove cache dir: {}", e))?;
        fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to recreate cache dir: {}", e))?;
    }

    info!("clear_thumbnail_cache done");
    Ok(())
}

/// 获取缓存大小（字节）
#[tauri::command]
pub fn get_thumbnail_cache_size(app: AppHandle) -> Result<u64, String> {
    let cache_dir = get_cache_dir(&app)?;

    if !cache_dir.exists() {
        return Ok(0);
    }

    let mut total_size = 0u64;
    for entry in fs::read_dir(&cache_dir).map_err(|e| format!("Failed to read cache dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            let metadata =
                fs::metadata(&path).map_err(|e| format!("Failed to get file metadata: {}", e))?;
            total_size += metadata.len();
        }
    }

    Ok(total_size)
}
