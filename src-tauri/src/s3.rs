use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use mime_guess::MimeGuess;
use rusty_s3::{Bucket, Credentials, S3Action};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3UploadResult {
    pub url: String,
    pub delete_id: String,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct S3DeleteResult {
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct S3DeleteMarker {
    bucket: String,
    region: String,
    key: String,
    endpoint: Option<String>,
    force_path_style: bool,
}

#[derive(Debug, Clone)]
struct S3ConfigOptions {
    region: String,
    endpoint: Option<String>,
    force_path_style: bool,
    access_key_id: String,
    secret_access_key: String,
}

fn build_bucket_and_credentials(
    options: &S3ConfigOptions,
    bucket_name: &str,
) -> Result<(Bucket, Credentials), String> {
    // 构建 endpoint
    let endpoint = if let Some(custom_endpoint) = &options.endpoint {
        // 去除路径部分，只保留 scheme 和 host
        let trimmed = custom_endpoint
            .splitn(4, '/')
            .take(3)
            .collect::<Vec<_>>()
            .join("/");
        trimmed
    } else {
        // 对于 AWS S3，使用标准的 endpoint 格式
        if options.force_path_style {
            format!("https://s3.{}.amazonaws.com", options.region)
        } else {
            format!(
                "https://{}.s3.{}.amazonaws.com",
                bucket_name, options.region
            )
        }
    };

    // 解析 endpoint
    let url = url::Url::parse(&endpoint).map_err(|err| format!("invalid endpoint URL: {}", err))?;

    // 创建 bucket
    let bucket = Bucket::new(
        url,
        if options.force_path_style {
            rusty_s3::UrlStyle::Path
        } else {
            rusty_s3::UrlStyle::VirtualHost
        },
        bucket_name.to_string(),
        options.region.clone(),
    )
    .map_err(|err| format!("failed to create bucket: {}", err))?;

    // 创建 credentials
    let credentials = Credentials::new(
        options.access_key_id.clone(),
        options.secret_access_key.clone(),
    );

    Ok((bucket, credentials))
}

fn sanitize_file_name(input: &str) -> String {
    let trimmed = input.trim();
    let fallback = "upload.bin";
    let candidate = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    candidate
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect()
}

fn generate_object_key(prefix: Option<&str>, original_name: &str) -> String {
    let sanitized = sanitize_file_name(original_name);
    let uuid = Uuid::new_v4();
    let date_prefix = Utc::now().format("%Y/%m/%d");
    let mut segments = Vec::new();
    if let Some(custom_prefix) = prefix {
        let trimmed = custom_prefix.trim_matches('/');
        if !trimmed.is_empty() {
            segments.push(trimmed.to_string());
        }
    }
    segments.push(date_prefix.to_string());
    segments.push(format!("{}-{}", uuid, sanitized));
    segments.join("/")
}

fn resolve_content_type(file_name: &str) -> Option<String> {
    let guess: MimeGuess = mime_guess::from_path(file_name);
    guess
        .first_raw()
        .map(|mime| mime.trim().to_string())
        .filter(|mime| !mime.is_empty())
}

fn build_public_url(
    public_base: Option<&str>,
    endpoint: Option<&str>,
    bucket: &str,
    region: &str,
    key: &str,
    force_path_style: bool,
) -> String {
    if let Some(base) = public_base {
        let trimmed = base.trim_end_matches('/');
        return format!("{}/{}", trimmed, key);
    }

    if let Some(endpoint) = endpoint {
        let trimmed = endpoint.trim_end_matches('/');
        if force_path_style {
            return format!("{}/{}/{}", trimmed, bucket, key);
        }
        return format!("{}/{}", trimmed, key);
    }

    if force_path_style {
        format!("https://s3.{}.amazonaws.com/{}/{}", region, bucket, key)
    } else {
        format!("https://{}.s3.{}.amazonaws.com/{}", bucket, region, key)
    }
}

fn map_acl(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let lowered = trimmed.to_ascii_lowercase();
    let acl = match lowered.as_str() {
        "private" => "private",
        "public-read" => "public-read",
        "public-read-write" => "public-read-write",
        "authenticated-read" => "authenticated-read",
        "aws-exec-read" => "aws-exec-read",
        "bucket-owner-read" => "bucket-owner-read",
        "bucket-owner-full-control" => "bucket-owner-full-control",
        other => {
            return Err(format!("unsupported ACL value: {other}"));
        }
    };
    Ok(Some(acl.to_string()))
}

#[tauri::command]
pub async fn s3_upload(
    file_path: String,
    original_file_name: String,
    bucket: String,
    region: String,
    access_key_id: String,
    secret_access_key: String,
    endpoint: Option<String>,
    force_path_style: Option<bool>,
    object_prefix: Option<String>,
    acl: Option<String>,
    public_base_url: Option<String>,
) -> Result<S3UploadResult, String> {
    let path = Path::new(&file_path);
    if !path.is_absolute() || !path.exists() {
        return Err("file path must be an existing absolute path".to_string());
    }

    let file_path_for_read = file_path.clone();
    let file_bytes =
        tauri::async_runtime::spawn_blocking(move || std::fs::read(&file_path_for_read))
            .await
            .map_err(|err| format!("failed to join file read task: {err}"))?
            .map_err(|err| format!("failed to read file: {err}"))?;

    let options = S3ConfigOptions {
        region: region.clone(),
        endpoint: endpoint.clone(),
        // default to path style when custom endpoint (e.g., Cloudflare R2) is used
        force_path_style: force_path_style.unwrap_or(endpoint.is_some()),
        access_key_id,
        secret_access_key,
    };

    let (bucket_obj, credentials) = build_bucket_and_credentials(&options, &bucket)
        .map_err(|err| format!("failed to build bucket and credentials: {}", err))?;

    let object_key = generate_object_key(object_prefix.as_deref(), &original_file_name);

    // 创建 PUT 操作
    let action = bucket_obj.put_object(Some(&credentials), &object_key);

    // 预签名时会由 `sign(Duration)` 添加过期参数，避免重复插入

    // 不将可变请求头加入到签名内（避免因 header 值或大小写差异导致 SignatureDoesNotMatch）。
    // 我们将在发起 HTTP 请求时，将 Content-Type 与 x-amz-acl 附加到 reqwest 请求头中。
    let content_type_header = resolve_content_type(&original_file_name);
    let acl_header = map_acl(acl.as_deref())?;

    // 生成预签名 URL
    let presigned_url = action.sign(Duration::from_secs(900));

    // 使用 reqwest 执行上传
    let client = reqwest::Client::new();
    let mut req = client.put(presigned_url.as_str()).body(file_bytes);
    if let Some(ct) = content_type_header {
        req = req.header("Content-Type", ct);
    }
    if let Some(acl_val) = acl_header {
        req = req.header("x-amz-acl", acl_val);
    }
    let response = req
        .send()
        .await
        .map_err(|err| format!("failed to upload file: {}", err))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "upload failed with status {}: {}",
            status, error_text
        ));
    }

    let delete_marker = S3DeleteMarker {
        bucket: bucket.clone(),
        region: region.clone(),
        key: object_key.clone(),
        endpoint,
        force_path_style: options.force_path_style,
    };

    // 对于 rusty-s3，我们无法直接从响应中获取 ETag 和 VersionId
    // 你可以选择从响应头中提取，或者省略这些元数据
    let metadata = None;

    let public_url = build_public_url(
        public_base_url.as_deref(),
        delete_marker.endpoint.as_deref(),
        &delete_marker.bucket,
        &delete_marker.region,
        &delete_marker.key,
        delete_marker.force_path_style,
    );

    let delete_id = serde_json::to_string(&delete_marker)
        .map_err(|err| format!("failed to serialize delete marker: {err}"))?;

    Ok(S3UploadResult {
        url: public_url,
        delete_id,
        metadata,
    })
}

#[tauri::command]
pub async fn s3_delete(
    delete_id: String,
    access_key_id: String,
    secret_access_key: String,
) -> Result<S3DeleteResult, String> {
    let marker: S3DeleteMarker = serde_json::from_str(&delete_id)
        .map_err(|err| format!("invalid deleteId payload: {err}"))?;

    let options = S3ConfigOptions {
        region: marker.region.clone(),
        endpoint: marker.endpoint.clone(),
        force_path_style: marker.force_path_style,
        access_key_id,
        secret_access_key,
    };

    let (bucket_obj, credentials) = build_bucket_and_credentials(&options, &marker.bucket)
        .map_err(|err| format!("failed to build bucket and credentials: {}", err))?;

    // 创建 DELETE 操作
    let action = bucket_obj.delete_object(Some(&credentials), &marker.key);

    // 预签名时会由 `sign(Duration)` 添加过期参数，避免重复插入

    // 生成预签名 URL
    let presigned_url = action.sign(Duration::from_secs(900));

    // 使用 reqwest 执行删除
    let client = reqwest::Client::new();
    let response = client
        .delete(presigned_url.as_str())
        .send()
        .await
        .map_err(|err| format!("failed to delete object: {}", err))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "delete failed with status {}: {}",
            status, error_text
        ));
    }

    Ok(S3DeleteResult {
        success: true,
        message: Some("对象已从 S3 删除".to_string()),
    })
}
