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
use std::sync::atomic::{AtomicBool, Ordering};

use log::{debug, error, info, warn};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

// 全局生成缩略图互斥锁：确保同时只有一个任务在执行
// 防止频繁切回导致的任务堆积
static GENERATING_THUMBNAILS: AtomicBool = AtomicBool::new(false);

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

/// 计算 URL 的 SHA256 哈希（取前 16 个十六进制字符）
/// 相比 DefaultHasher 提供更好的碰撞抵抗性和跨平台一致性
/// 16 字符 = 64 位空间（2^64 ≈ 1.8 × 10^19，极低碰撞概率）
fn compute_url_hash(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)[0..16].to_string()
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

/// 下载图片到指定路径（异步 I/O，带重试机制和自适应策略）
async fn download_image(url: &str, dest_path: &PathBuf) -> Result<u64, String> {
    const MAX_RETRIES: u32 = 3;
    const INITIAL_TIMEOUT_SECS: u64 = 30;
    const MAX_SIZE: u64 = 50 * 1024 * 1024; // 50MB 限制

    let mut last_error = String::new();

    for attempt in 1..=MAX_RETRIES {
        debug!(
            "Downloading image from URL: {} (attempt {}/{})",
            url, attempt, MAX_RETRIES
        );

        // 根据重试次数调整超时时间（逐次增加）
        let timeout_secs = INITIAL_TIMEOUT_SECS + (attempt as u64 - 1) * 10;

        match download_image_attempt(url, dest_path, timeout_secs, MAX_SIZE).await {
            Ok(size) => {
                if attempt > 1 {
                    info!(
                        "Downloaded successfully on attempt {}/{}: {}",
                        attempt, MAX_RETRIES, url
                    );
                }
                return Ok(size);
            }
            Err(e) => {
                last_error = e.clone();

                // 判断是否可重试
                if should_retry(&e, attempt, MAX_RETRIES) {
                    // 指数退避：1秒、2秒、4秒等待
                    let wait_time = 1000 * 2_u64.pow(attempt - 1);
                    debug!(
                        "Download failed ({}), retrying in {}ms: {}",
                        attempt, wait_time, e
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(wait_time)).await;
                } else {
                    // 非可重试错误，立即返回
                    return Err(e);
                }
            }
        }
    }

    Err(format!(
        "Failed to download {} after {} attempts: {}",
        url, MAX_RETRIES, last_error
    ))
}

/// 判断错误是否应该重试
fn should_retry(error: &str, attempt: u32, max_retries: u32) -> bool {
    if attempt >= max_retries {
        return false;
    }

    // 永不重试的错误
    let permanent_errors = [
        "HTTP error 404",            // 不存在
        "HTTP error 403",            // 禁止访问
        "HTTP error 401",            // 未授权
        "Downloaded image is empty", // 空文件
        "Image too large",           // 文件过大
        "Failed to open image",      // 图片格式错误
        "Failed to save thumbnail",  // 缩略图保存失败
    ];

    for permanent in &permanent_errors {
        if error.contains(permanent) {
            return false;
        }
    }

    // 可重试的错误
    let retriable_errors = [
        "Failed to read response", // 响应读取错误
        "Failed to download",      // 下载错误
        "timed out",               // 超时
        "connection",              // 连接问题
        "Failed to write to file", // 临时写入错误
        "error decoding",          // 编码错误
        "interrupted",             // 中断
    ];

    for retriable in &retriable_errors {
        if error.to_lowercase().contains(retriable) {
            return true;
        }
    }

    // 默认可重试（网络波动等未知错误）
    true
}

/// 单次下载尝试（不包含重试逻辑）
async fn download_image_attempt(
    url: &str,
    dest_path: &PathBuf,
    timeout_secs: u64,
    max_size: u64,
) -> Result<u64, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .header("Accept-Encoding", "identity") // 禁用自动解码，手动处理
        .timeout(std::time::Duration::from_secs(timeout_secs))
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

    // 检查 Content-Type，确保是图片
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !is_valid_image_content_type(content_type) {
        debug!(
            "Suspicious Content-Type: {}, still attempting to download",
            content_type
        );
    }

    // 获取 Content-Length 以防止下载过大的文件
    if let Some(content_length) = response.content_length() {
        if content_length > max_size {
            return Err(format!(
                "Image too large ({} bytes) from {} (max: {} bytes)",
                content_length, url, max_size
            ));
        }
    }

    // 读取响应体
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body from {}: {}", url, e))?;

    let file_size = bytes.len() as u64;

    // 检查是否为空
    if file_size == 0 {
        return Err(format!("Downloaded image is empty from {}", url));
    }

    // 防止下载超过 50MB（再次检查）
    if file_size > max_size {
        return Err(format!(
            "Downloaded data exceeds maximum size ({} bytes) from {}",
            max_size, url
        ));
    }

    // 基础的图片格式验证（检查魔数）
    if !is_valid_image_magic(&bytes) {
        return Err(format!(
            "Downloaded file does not appear to be a valid image from {}",
            url
        ));
    }

    let mut file = std::fs::File::create(dest_path)
        .map_err(|e| format!("Failed to create file {}: {}", dest_path.display(), e))?;

    file.write_all(&bytes)
        .map_err(|e| format!("Failed to write to file: {}", e))?;

    debug!(
        "Downloaded image to: {}, size: {} bytes",
        dest_path.display(),
        file_size
    );
    Ok(file_size)
}

/// 检查是否为有效的图片 Content-Type
fn is_valid_image_content_type(content_type: &str) -> bool {
    let valid_types = [
        "image/jpeg",
        "image/png",
        "image/webp",
        "image/gif",
        "image/bmp",
        "image/tiff",
        "image/svg+xml",
    ];

    for valid in &valid_types {
        if content_type.contains(valid) {
            return true;
        }
    }

    // 某些服务器可能返回 application/octet-stream 等
    content_type.is_empty() || content_type.contains("application/octet-stream")
}

/// 检查文件是否为有效的图片（通过魔数）
fn is_valid_image_magic(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }

    // 常见图片格式的魔数
    let valid_magics = [
        &[0xFF, 0xD8, 0xFF][..],       // JPEG
        &[0x89, 0x50, 0x4E, 0x47][..], // PNG
        &[0x47, 0x49, 0x46][..],       // GIF
        &[0x42, 0x4D][..],             // BMP
        &[0x52, 0x49, 0x46, 0x46][..], // RIFF (WebP/AVI)
        &[0x49, 0x49, 0x2A, 0x00][..], // TIFF (little-endian)
        &[0x4D, 0x4D, 0x00, 0x2A][..], // TIFF (big-endian)
        &[0x3C, 0x73, 0x76, 0x67][..], // SVG (<svg)
    ];

    for magic in &valid_magics {
        if data.len() >= magic.len() && data.starts_with(magic) {
            return true;
        }
    }

    false
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
    // 尝试获取全局锁
    let is_locked =
        GENERATING_THUMBNAILS.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed);

    match is_locked {
        Ok(_) => {
            // ✅ 成功获取锁，继续处理
            debug!("Acquired thumbnail generation lock");
        }
        Err(_) => {
            // ❌ 另一个任务正在生成，直接返回（不阻塞）
            warn!(
                "Thumbnail generation already in progress, rejecting request with {} URLs",
                urls.len()
            );
            return Err(
                "Thumbnail generation is already in progress. Please try again later.".to_string(),
            );
        }
    }

    // 使用 defer 模式确保无论是否成功，都释放锁
    let result = generate_thumbnails_impl(app, urls).await;

    // 释放锁
    GENERATING_THUMBNAILS.store(false, Ordering::Release);
    debug!("Released thumbnail generation lock");

    result
}

/// 实际的缩略图生成实现
async fn generate_thumbnails_impl(
    app: AppHandle,
    urls: Vec<String>,
) -> Result<Vec<String>, String> {
    info!(
        "generate_thumbnails_impl start: count={}, urls={:?}",
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
            "generate_thumbnails_impl done: count={}, failed={}",
            output.len(),
            failed_count
        );
    } else {
        info!("generate_thumbnails_impl done: count={}", output.len());
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

/// 为已上传的图片生成缩略图（专门给上传界面用）
/// 接受 (url, local_file_path) 元组数组
/// 优势：不需要再次下载图片，直接使用本地文件压缩，减少性能消耗
#[tauri::command]
pub async fn generate_thumbnails_from_local(
    app: AppHandle,
    items: Vec<(String, String)>, // (url, local_file_path)
) -> Result<Vec<String>, String> {
    info!(
        "generate_thumbnails_from_local start: count={}",
        items.len()
    );

    // 尝试获取全局锁
    let is_locked =
        GENERATING_THUMBNAILS.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed);

    match is_locked {
        Ok(_) => {
            debug!("Acquired thumbnail generation lock");
        }
        Err(_) => {
            warn!(
                "Thumbnail generation already in progress, rejecting request with {} items",
                items.len()
            );
            return Err(
                "Thumbnail generation is already in progress. Please try again later.".to_string(),
            );
        }
    }

    let result = generate_thumbnails_from_local_impl(app, items).await;

    // 释放锁
    GENERATING_THUMBNAILS.store(false, Ordering::Release);
    debug!("Released thumbnail generation lock");

    result
}

/// 实际的本地文件缩略图生成实现
async fn generate_thumbnails_from_local_impl(
    app: AppHandle,
    items: Vec<(String, String)>,
) -> Result<Vec<String>, String> {
    let cache_dir = get_cache_dir(&app)?;

    // 创建任务列表：(url, local_path) -> 处理任务
    let mut tasks = Vec::new();
    for (url, local_path) in items {
        let cache_dir_clone = cache_dir.clone();
        tasks.push(process_thumbnail_from_local(
            url,
            local_path,
            cache_dir_clone,
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
                error!(
                    "Failed to generate thumbnail from local for index {}: {}",
                    idx, e
                );
            }
        }
    }

    if failed_count > 0 && output.is_empty() {
        return Err(format!(
            "Failed to generate all {} thumbnails from local files",
            failed_count
        ));
    }

    if failed_count > 0 {
        info!(
            "generate_thumbnails_from_local_impl done: count={}, failed={}",
            output.len(),
            failed_count
        );
    } else {
        info!(
            "generate_thumbnails_from_local_impl done: count={}",
            output.len()
        );
    }

    Ok(output)
}

/// 从本地文件生成单个缩略图
async fn process_thumbnail_from_local(
    url: String,
    local_path: String,
    cache_dir: PathBuf,
) -> Result<String, String> {
    debug!(
        "Processing thumbnail from local file: {} (url: {})",
        local_path, url
    );

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

    // 检查本地文件是否存在
    let file_path = PathBuf::from(&local_path);
    if !file_path.exists() {
        return Err(format!("Local file not found: {}", local_path));
    }

    // 验证是否为有效的图片文件
    let file_data = fs::read(&file_path)
        .map_err(|e| format!("Failed to read local file {}: {}", local_path, e))?;

    if !is_valid_image_magic(&file_data) {
        return Err(format!(
            "Local file does not appear to be a valid image: {}",
            local_path
        ));
    }

    // 如果本地文件没有扩展名或扩展名不对，创建一个带正确后缀的临时文件副本
    // 这是为了确保 image library 能正确识别文件格式
    let input_file = if let Some(ext) = file_path.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        // 检查是否是已知的图片格式扩展名
        let known_exts = [
            "jpg", "jpeg", "png", "webp", "gif", "bmp", "tiff", "tif", "svg",
        ];
        if known_exts.contains(&ext_str.as_str()) {
            // 扩展名正确，直接使用原文件
            file_path.clone()
        } else {
            // 扩展名不被识别，需要创建临时文件
            create_temp_file_with_extension(&file_path, &file_data, &url)?
        }
    } else {
        // 没有扩展名，需要从 URL 推断并创建临时文件
        create_temp_file_with_extension(&file_path, &file_data, &url)?
    };

    // 压缩为缩略图
    let thumbnail_size = compress_to_thumbnail(&input_file, &cache_path)?;

    // 如果创建了临时文件，需要清理它
    if input_file != file_path {
        if let Err(e) = fs::remove_file(&input_file) {
            error!(
                "Failed to remove temporary file {}: {}",
                input_file.display(),
                e
            );
        }
    }

    info!(
        "Thumbnail generated from local file: {} (thumbnail: {} bytes, url: {})",
        cache_path.to_string_lossy(),
        thumbnail_size,
        url
    );

    Ok(cache_path.to_string_lossy().to_string())
}

/// 创建一个带正确后缀名的临时文件副本
/// 这样 image library 能根据扩展名正确识别文件格式
fn create_temp_file_with_extension(
    original_path: &PathBuf,
    file_data: &[u8],
    url: &str,
) -> Result<PathBuf, String> {
    let temp_dir = app_temp_dir()?;

    // 从 URL 推断扩展名
    let ext = extract_file_extension(url);
    let temp_path = temp_dir.join(format!("thumb_{}{}", uuid::Uuid::new_v4(), ext));

    // 将文件数据写入临时文件
    let mut file = std::fs::File::create(&temp_path).map_err(|e| {
        format!(
            "Failed to create temporary file {}: {}",
            temp_path.display(),
            e
        )
    })?;

    file.write_all(file_data)
        .map_err(|e| format!("Failed to write to temporary file: {}", e))?;

    debug!(
        "Created temporary file with extension: {} (from: {})",
        temp_path.display(),
        original_path.display()
    );

    Ok(temp_path)
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
