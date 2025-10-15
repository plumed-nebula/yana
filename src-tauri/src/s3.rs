use std::path::Path;

use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
use aws_sdk_s3::{
    Client,
    config::{Builder as S3ConfigBuilder, Region},
    error::{ProvideErrorMetadata, SdkError},
    primitives::ByteStream,
    types::ObjectCannedAcl,
};
use chrono::Utc;
use mime_guess::MimeGuess;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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

async fn build_client(options: &S3ConfigOptions) -> Result<Client, String> {
    let region = if options.endpoint.is_some() {
        // Cloudflare R2 requires signing region "auto"
        Region::new("auto".to_string())
    } else {
        Region::new(options.region.clone())
    };
    let credentials = Credentials::new(
        options.access_key_id.clone(),
        options.secret_access_key.clone(),
        None,
        None,
        "Static",
    );
    let provider = SharedCredentialsProvider::new(credentials);

    // load config with region for signing
    let shared_config = aws_config::from_env()
        .region(region.clone())
        .credentials_provider(provider)
        .load()
        .await;

    // inherit region from shared_config for signing
    let mut builder = S3ConfigBuilder::from(&shared_config);
    if let Some(endpoint) = &options.endpoint {
        // strip any path segments, keep only scheme and host
        let trimmed = endpoint
            .splitn(4, '/')
            .take(3)
            .collect::<Vec<_>>()
            .join("/");
        builder = builder.endpoint_url(&trimmed);
    }
    if options.force_path_style {
        builder = builder.force_path_style(true);
    }

    let config = builder.build();
    Ok(Client::from_conf(config))
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

fn map_acl(value: Option<&str>) -> Result<Option<ObjectCannedAcl>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let lowered = trimmed.to_ascii_lowercase();
    let acl = match lowered.as_str() {
        "private" => ObjectCannedAcl::Private,
        "public-read" => ObjectCannedAcl::PublicRead,
        "public-read-write" => ObjectCannedAcl::PublicReadWrite,
        "authenticated-read" => ObjectCannedAcl::AuthenticatedRead,
        "aws-exec-read" => ObjectCannedAcl::AwsExecRead,
        "bucket-owner-read" => ObjectCannedAcl::BucketOwnerRead,
        "bucket-owner-full-control" => ObjectCannedAcl::BucketOwnerFullControl,
        other => {
            return Err(format!("unsupported ACL value: {other}"));
        }
    };
    Ok(Some(acl))
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
    let client = build_client(&options).await?;

    let object_key = generate_object_key(object_prefix.as_deref(), &original_file_name);
    let mut request = client
        .put_object()
        .bucket(&bucket)
        .key(&object_key)
        .body(ByteStream::from(file_bytes));

    if let Some(content_type) = resolve_content_type(&original_file_name) {
        request = request.content_type(content_type);
    }

    // set ACL if provided
    if let Some(acl_val) = map_acl(acl.as_deref())? {
        request = request.acl(acl_val);
    }

    let response = request.send().await.map_err(|err| {
        let detail = if let SdkError::ServiceError(service_err) = &err {
            let code = service_err
                .err()
                .code()
                .map(str::to_string)
                .unwrap_or_else(|| "Unknown".to_string());
            let message = service_err
                .err()
                .message()
                .map(str::to_string)
                .unwrap_or_else(|| service_err.err().to_string());
            format!("service error (code {code}): {message}")
        } else if let SdkError::ResponseError(response_err) = &err {
            format!("response error: {response_err:?}")
        } else if let SdkError::DispatchFailure(failure) = &err {
            format!("dispatch failure: {failure:?}")
        } else if let SdkError::TimeoutError(timeout) = &err {
            format!("timeout: {timeout:?}")
        } else if let SdkError::ConstructionFailure(construction) = &err {
            format!("construction failure: {construction:?}")
        } else {
            err.to_string()
        };
        format!("failed to upload to S3: {detail}")
    })?;

    let delete_marker = S3DeleteMarker {
        bucket: bucket.clone(),
        region: region.clone(),
        key: object_key.clone(),
        endpoint,
        force_path_style: options.force_path_style,
    };

    let mut metadata_map = serde_json::Map::new();
    if let Some(etag) = response.e_tag() {
        metadata_map.insert("etag".to_string(), json!(etag));
    }
    if let Some(version_id) = response.version_id() {
        metadata_map.insert("versionId".to_string(), json!(version_id));
    }
    let metadata = if metadata_map.is_empty() {
        None
    } else {
        Some(Value::Object(metadata_map))
    };

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
    let client = build_client(&options).await?;

    client
        .delete_object()
        .bucket(&marker.bucket)
        .key(&marker.key)
        .send()
        .await
        .map_err(|err| {
            let detail = if let SdkError::ServiceError(service_err) = &err {
                let code = service_err
                    .err()
                    .code()
                    .map(str::to_string)
                    .unwrap_or_else(|| "Unknown".to_string());
                let message = service_err
                    .err()
                    .message()
                    .map(str::to_string)
                    .unwrap_or_else(|| service_err.err().to_string());
                format!("service error (code {code}): {message}")
            } else if let SdkError::ResponseError(response_err) = &err {
                format!("response error: {response_err:?}")
            } else if let SdkError::DispatchFailure(failure) = &err {
                format!("dispatch failure: {failure:?}")
            } else if let SdkError::TimeoutError(timeout) = &err {
                format!("timeout: {timeout:?}")
            } else if let SdkError::ConstructionFailure(construction) = &err {
                format!("construction failure: {construction:?}")
            } else {
                err.to_string()
            };
            format!("failed to delete S3 object: {detail}")
        })?;

    Ok(S3DeleteResult {
        success: true,
        message: Some("对象已从 S3 删除".to_string()),
    })
}
