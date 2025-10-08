use std::{collections::HashMap, path::Path, time::Duration};

use base64::{Engine as _, engine::general_purpose};
use reqwest::{
    Client, Response,
    header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadFormat {
    /// 以二进制请求体直接上传
    Binary,
    /// 以 multipart/form-data 上传
    Form,
    /// 将文件编码为 base64 并包裹在 JSON 中上传
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadConfig {
    /// 上传目标地址
    pub url: String,
    /// 自定义请求头
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// form-data 模式下的文件字段名
    #[serde(default = "default_field_name")]
    pub field_name: String,
    /// form-data 模式下的额外文本字段
    #[serde(default)]
    pub additional_fields: HashMap<String, String>,
    /// base64 模式下图片字段的键名，默认 image
    #[serde(default)]
    pub json_key: Option<String>,
    /// base64 模式下的额外 JSON 字段
    #[serde(default)]
    pub additional_json: HashMap<String, serde_json::Value>,
    /// 自定义文件名，默认使用文件路径中的文件名
    #[serde(default)]
    pub file_name: Option<String>,
    /// 指定 Content-Type，不指定则默认为 application/octet-stream
    #[serde(default)]
    pub content_type: Option<String>,
    /// 请求超时时间，单位毫秒，默认 30 秒
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn default_field_name() -> String {
    "file".to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    pub raw_text: String,
}

#[tauri::command]
pub async fn upload_image(
    file_path: String,
    format: UploadFormat,
    config: UploadConfig,
) -> Result<UploadResponse, String> {
    let path = Path::new(&file_path);
    if !path.is_absolute() || !path.exists() {
        return Err("file path must be an existing absolute path".to_string());
    }

    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("parent directory segments are not allowed in file path".to_string());
    }

    let default_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload.bin")
        .to_string();

    let UploadConfig {
        url,
        headers,
        field_name,
        additional_fields,
        json_key,
        additional_json,
        file_name,
        content_type,
        timeout_ms,
    } = config;

    let effective_file_name = file_name.unwrap_or(default_name);

    let file_path_for_read = file_path.clone();
    let file_bytes =
        tauri::async_runtime::spawn_blocking(move || std::fs::read(file_path_for_read))
            .await
            .map_err(|e| format!("failed to join file read task: {}", e))
            .and_then(|res| res.map_err(|e| format!("failed to read file: {}", e)))?;

    let timeout = timeout_ms.unwrap_or(30_000);
    let client = Client::builder()
        .timeout(Duration::from_millis(timeout))
        .build()
        .map_err(|e| format!("failed to build http client: {}", e))?;

    let header_map = build_header_map(&headers)?;

    let response = match format {
        UploadFormat::Binary => {
            let mut request = client.post(&url).headers(header_map.clone());
            if let Some(ct) = &content_type {
                request = request.header(CONTENT_TYPE, ct);
            } else {
                request = request.header(CONTENT_TYPE, "application/octet-stream");
            }
            request
                .body(file_bytes)
                .send()
                .await
                .map_err(|e| format!("failed to send binary upload request: {}", e))?
        }
        UploadFormat::Form => {
            let mut part =
                reqwest::multipart::Part::bytes(file_bytes).file_name(effective_file_name.clone());
            if let Some(ct) = &content_type {
                part = part
                    .mime_str(ct)
                    .map_err(|e| format!("invalid content type `{}`: {}", ct, e))?;
            }

            let mut form = reqwest::multipart::Form::new().part(field_name, part);
            for (key, value) in additional_fields {
                form = form.text(key, value);
            }

            client
                .post(&url)
                .headers(header_map.clone())
                .multipart(form)
                .send()
                .await
                .map_err(|e| format!("failed to send form upload request: {}", e))?
        }
        UploadFormat::Base64 => {
            let encoded = general_purpose::STANDARD.encode(&file_bytes);
            let key = json_key.unwrap_or_else(|| "image".to_string());
            let mut payload = serde_json::Map::new();
            payload.insert(key, serde_json::Value::String(encoded));

            for (k, v) in additional_json {
                payload.insert(k, v);
            }

            let request_body = serde_json::Value::Object(payload);

            client
                .post(&url)
                .headers(header_map.clone())
                .json(&request_body)
                .send()
                .await
                .map_err(|e| format!("failed to send base64 upload request: {}", e))?
        }
    };

    finalize_response(response).await
}

fn build_header_map(headers: &HashMap<String, String>) -> Result<HeaderMap, String> {
    let mut map = HeaderMap::new();
    for (key, value) in headers {
        let name = HeaderName::from_bytes(key.trim().as_bytes())
            .map_err(|e| format!("invalid header name `{}`: {}", key, e))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|e| format!("invalid header value for `{}`: {}", key, e))?;
        map.insert(name, header_value);
    }
    Ok(map)
}

async fn finalize_response(response: Response) -> Result<UploadResponse, String> {
    let status = response.status();
    let headers = response.headers().clone();
    let raw_text = response
        .text()
        .await
        .map_err(|e| format!("failed to read response body: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "upload failed with status {}: {}",
            status.as_u16(),
            raw_text
        ));
    }

    let parsed_body = serde_json::from_str(&raw_text).unwrap_or(serde_json::Value::Null);
    let header_pairs = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    Ok(UploadResponse {
        status: status.as_u16(),
        headers: header_pairs,
        body: parsed_body,
        raw_text,
    })
}
