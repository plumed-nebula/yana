/*
模块职责概述（后端图片压缩/转码）：
1) 读取文件字节并根据文件头准确判断真实格式；
2) 区分静态图与动图（GIF 通过逐帧检测，WebP 通过 ANIM chunk 进行启发式判断）；
3) 静态图：按照“原格式”或“WebP”两种目标模式分别编码。
    - PNG：无损编码，使用压缩级别映射 quality，quality 越低压缩越强（更慢）。
    - JPEG：有损编码，直接使用 quality（0-100）。
    - WebP（静态）：使用 webp crate 支持可调质量。
    - 其他格式（BMP/TIFF/PNM/TGA/ICO）：回退到 image 的通用写入。
4) 动图：
    - GIF：重新逐帧编码为 GIF；若目标为 WebP，当前回退为“首帧静态 WebP”。
    - 动画 WebP：暂时原样透传（保持动画）；若目标为 WebP 同样透传。
5) 输出：使用 tempfile 在系统临时目录生成输出文件，返回绝对路径（顺序与输入一致）。
6) 并行：使用 rayon 并发处理，最后按原始索引恢复顺序。

注意：image 目前对动画 WebP 的编码支持有限，因此 WebP 动画暂未重编码，仅保留原样或退化为首帧静态图。
*/

use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

use image::GenericImageView;
use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{
    CompressionType as PngCompressionType, FilterType as PngFilterType, PngEncoder,
};
use image::{
    self, AnimationDecoder, ColorType, DynamicImage, ImageEncoder, ImageFormat, ImageReader,
};
use log::{debug, error, info, warn};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tempfile::Builder as TempFileBuilder;
use webp::{Encoder as WebpEncoder, PixelLayout}; // adjustable-quality webp

// Helper: application-specific temp directory inside system temp
fn app_temp_dir() -> Result<PathBuf, String> {
    let mut dir = std::env::temp_dir();
    // prefer identifier from tauri.conf.json (compile-time include) and fallback to cargo package name
    // use fixed app identifier to avoid parsing config at compile time
    let identifier = "com.yana.dev".to_string();
    dir.push(identifier);
    Ok(dir)
}

fn ensure_app_temp_dir() -> Result<PathBuf, String> {
    let dir = app_temp_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create app temp dir {}: {}", dir.display(), e))?;
    Ok(dir)
}

fn cleanup_app_temp_dir_internal() -> Result<(), String> {
    let dir = app_temp_dir()?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("remove app temp dir {}: {}", dir.display(), e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn clean_app_temp_dir() -> Result<(), String> {
    info!("clean_app_temp_dir start");
    cleanup_app_temp_dir_internal()?;
    // recreate empty dir so subsequent tempfile_in calls succeed
    ensure_app_temp_dir()?;
    info!("clean_app_temp_dir done");
    Ok(())
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(non_camel_case_types)]
pub enum Mode {
    /// 输出为与输入一致的“原格式”（静态图会按原格式的编码器重编码；动图尽量保持原格式/动画）
    original_format,
    /// 输出为 WebP（静态图为可调质量的 WebP；动图目前回退为首帧静态 WebP，动画 WebP 原样透传）
    webp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PngCompressionMode {
    Lossy,
    Lossless,
}

impl Default for PngCompressionMode {
    fn default() -> Self {
        Self::Lossless
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PngOptimizationLevel {
    Best,
    Default,
    Fast,
}

impl Default for PngOptimizationLevel {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Clone)]
enum DetectedKind {
    /// 静态图（格式）
    Static(ImageFormat),
    /// 动图（格式）：GIF 或动画 WebP（WebP 通过 ANIM chunk 进行启发式判断）
    Animated(ImageFormat),
}

fn read_all_bytes(path: &str) -> Result<Vec<u8>, String> {
    // 直接以字节读取，后续用 guess_format 基于 header 判定真实格式
    debug!("read_all_bytes start: path={}", path);
    let mut f = File::open(path).map_err(|e| format!("open {}: {}", path, e))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| format!("read {}: {}", path, e))?;
    debug!("read_all_bytes done: path={}, bytes={}", path, buf.len());
    Ok(buf)
}

fn detect_format_and_kind(bytes: &[u8]) -> Result<DetectedKind, String> {
    let format = image::guess_format(bytes).map_err(|e| format!("guess format: {}", e))?;
    match format {
        ImageFormat::Gif => {
            // GIF：利用 GifDecoder into_frames() 尝试取两帧，若第二帧存在则认为是动图
            let decoder =
                GifDecoder::new(Cursor::new(bytes)).map_err(|e| format!("gif decode: {}", e))?;
            let mut frames = decoder.into_frames();
            let _ = frames.next();
            if frames.next().is_some() {
                debug!("detected animated gif");
                Ok(DetectedKind::Animated(ImageFormat::Gif))
            } else {
                debug!("detected static gif");
                Ok(DetectedKind::Static(ImageFormat::Gif))
            }
        }
        ImageFormat::WebP => {
            // WebP：暂未提供方便的动图检测 API，这里采用启发式：查找 ANIM chunk
            if bytes.windows(4).any(|w| w == b"ANIM") {
                debug!("detected animated webp");
                Ok(DetectedKind::Animated(ImageFormat::WebP))
            } else {
                debug!("detected static webp");
                Ok(DetectedKind::Static(ImageFormat::WebP))
            }
        }
        other => Ok(DetectedKind::Static(other)),
    }
}

// ---------- Static encoders ----------

fn encode_png(
    img: &DynamicImage,
    quality: u8,
    mode: PngCompressionMode,
    optimization: PngOptimizationLevel,
) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::new());
    let compression = match optimization {
        PngOptimizationLevel::Best => PngCompressionType::Best,
        PngOptimizationLevel::Default => PngCompressionType::Default,
        PngOptimizationLevel::Fast => PngCompressionType::Fast,
    };
    let filter = PngFilterType::Sub;
    let mut rgba = img.to_rgba8();

    if mode == PngCompressionMode::Lossy {
        let step = match quality {
            0..=10 => 48u8,
            11..=25 => 32u8,
            26..=45 => 16u8,
            46..=65 => 8u8,
            66..=85 => 4u8,
            86..=95 => 2u8,
            _ => 1u8,
        };
        if step > 1 {
            for pixel in rgba.pixels_mut() {
                pixel.0[0] = quantize_channel(pixel.0[0], step);
                pixel.0[1] = quantize_channel(pixel.0[1], step);
                pixel.0[2] = quantize_channel(pixel.0[2], step);
            }
        }
    }

    let encoder = PngEncoder::new_with_quality(&mut cursor, compression, filter);
    let (w, h) = rgba.dimensions();
    encoder
        .write_image(&rgba, w, h, ColorType::Rgba8.into())
        .map_err(|e| format!("png encode: {}", e))?;

    Ok(cursor.into_inner())
}

fn quantize_channel(value: u8, step: u8) -> u8 {
    if step <= 1 {
        return value;
    }
    let step = step as u16;
    let value = value as u16;
    let rounded = ((value + step / 2) / step) * step;
    rounded.min(255) as u8
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    // JPEG 为有损：quality 直接决定画质（0-100）
    let mut cursor = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut cursor, quality);

    // 根据原始色彩模式选择编码路径，避免不必要的转换
    match img {
        DynamicImage::ImageLuma8(gray) => {
            encoder
                .write_image(
                    gray.as_raw(),
                    gray.width(),
                    gray.height(),
                    ColorType::L8.into(),
                )
                .map_err(|e| format!("jpeg luma8 encode: {}", e))?;
        }
        DynamicImage::ImageRgb8(rgb) => {
            encoder
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    ColorType::Rgb8.into(),
                )
                .map_err(|e| format!("jpeg rgb8 encode: {}", e))?;
        }
        _ => {
            // 对于其他格式（如 RGBA），回退到 RGB8
            let rgb = img.to_rgb8();
            encoder
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    ColorType::Rgb8.into(),
                )
                .map_err(|e| format!("jpeg fallback encode: {}", e))?;
        }
    }

    Ok(cursor.into_inner())
}

fn encode_webp_static(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let has_alpha = img.color().has_alpha();

    let layout = if has_alpha {
        PixelLayout::Rgba
    } else {
        PixelLayout::Rgb
    };

    let (width, height) = img.dimensions();

    let encoder = WebpEncoder::new(img.as_bytes(), layout, width, height);

    let output = encoder.encode(quality as f32);

    Ok(output.to_vec())
}

fn encode_to_format(
    img: &DynamicImage,
    format: ImageFormat,
    quality: u8,
    png_mode: PngCompressionMode,
    png_optimization: PngOptimizationLevel,
) -> Result<Vec<u8>, String> {
    match format {
        ImageFormat::Png => encode_png(img, quality, png_mode, png_optimization),
        ImageFormat::Jpeg => encode_jpeg(img, quality),
        ImageFormat::WebP => encode_webp_static(img, quality),
        ImageFormat::Bmp
        | ImageFormat::Tiff
        | ImageFormat::Pnm
        | ImageFormat::Tga
        | ImageFormat::Ico => {
            // 这些格式使用 image 的通用写入作为回退
            let mut cursor = Cursor::new(Vec::new());
            img.write_to(&mut cursor, format)
                .map_err(|e| format!("encode {:?}: {}", format, e))?;
            Ok(cursor.into_inner())
        }
        other => {
            let mut cursor = Cursor::new(Vec::new());
            img.write_to(&mut cursor, other)
                .map_err(|e| format!("encode {:?}: {}", other, e))?;
            Ok(cursor.into_inner())
        }
    }
}

// ---------- Animated encoders ----------

fn reencode_gif_frames(bytes: &[u8], _quality: u8) -> Result<Vec<u8>, String> {
    // GIF 逐帧重编码；目前未进行颜色量化/抖动的深度控制，保持尽量接近原动画
    let decoder = GifDecoder::new(Cursor::new(bytes)).map_err(|e| format!("gif decode: {}", e))?;
    let mut frames = decoder.into_frames();
    let mut out = Cursor::new(Vec::new());
    {
        let mut enc = GifEncoder::new(&mut out);
        enc.set_repeat(Repeat::Infinite)
            .map_err(|e| format!("gif repeat: {}", e))?;
        while let Some(frame) = frames.next() {
            let f = frame.map_err(|e| format!("gif frame: {}", e))?;
            enc.encode_frame(f)
                .map_err(|e| format!("gif encode frame: {}", e))?;
        }
    }
    Ok(out.into_inner())
}

fn animated_to_webp(bytes: &[u8], quality: u8) -> Result<Vec<u8>, String> {
    // 当前缺少动画 WebP 编码：回退为“首帧静态 WebP”
    let img = image::load_from_memory(bytes).map_err(|e| format!("decode first frame: {}", e))?;
    encode_webp_static(&img, quality)
}

// ---------- Orchestrator ----------

fn process_one(
    path: &str,
    quality: u8,
    mode: Mode,
    force_animated_webp: bool,
    png_mode: PngCompressionMode,
    png_optimization: PngOptimizationLevel,
) -> Result<PathBuf, String> {
    info!(
        "process_one start: path={}, quality={}, mode={:?}, force_animated_webp={}, png_mode={:?}, png_optimization={:?}",
        path, quality, mode, force_animated_webp, png_mode, png_optimization
    );
    // 读取并判定格式/动图属性
    let bytes = read_all_bytes(path)?;
    let kind = detect_format_and_kind(&bytes)?;

    // 在应用专属临时目录创建输出文件（实际路径在 keep() 之后可被持久化）
    let tmp_dir = ensure_app_temp_dir()?;
    let mut tmp = TempFileBuilder::new()
        .prefix("yana_")
        .suffix("")
        .tempfile_in(&tmp_dir)
        .map_err(|e| format!("tempfile_in: {}", e))?;

    match (kind, mode) {
        (DetectedKind::Static(fmt), Mode::original_format) => {
            let img = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| format!("reader: {}", e))?
                .decode()
                .map_err(|e| format!("decode: {}", e))?;
            let out = encode_to_format(&img, fmt, quality, png_mode, png_optimization)?;
            tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
        }
        (DetectedKind::Static(_), Mode::webp) => {
            let img = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| format!("reader: {}", e))?
                .decode()
                .map_err(|e| format!("decode: {}", e))?;
            let out = encode_webp_static(&img, quality)?;
            tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
        }
        (DetectedKind::Animated(fmt), _) => {
            if force_animated_webp {
                // 强制将动图转为 WebP：当前实现能力有限，发出警告
                warn!(
                    "animated-to-webp is enabled: path={}, detected_format={:?}; result may not meet expectations",
                    path, fmt
                );
                match fmt {
                    ImageFormat::WebP => {
                        // 已是 WebP（包含动画 WebP）：直接透传
                        tmp.write_all(&bytes).map_err(|e| format!("write: {}", e))?;
                    }
                    _ => {
                        // 回退：取首帧静态并编码为 WebP
                        let out = animated_to_webp(&bytes, quality)?;
                        tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
                    }
                }
            } else {
                // 普通动画化压缩：GIF 重编码为 GIF；动画 WebP 或未知动画保持原样
                match fmt {
                    ImageFormat::Gif => {
                        let out = reencode_gif_frames(&bytes, quality)?;
                        tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
                    }
                    _ => {
                        tmp.write_all(&bytes).map_err(|e| format!("write: {}", e))?;
                    }
                }
            }
        }
    }

    // keep() 将临时文件持久化并返回 PathBuf
    let path_buf = tmp
        .into_temp_path()
        .keep()
        .map_err(|e| format!("keep temp: {}", e))?;
    info!(
        "process_one done: path={}, output={}",
        path,
        path_buf.display()
    );
    Ok(path_buf)
}

#[tauri::command]
pub async fn compress_images(
    paths: Vec<String>,
    quality: u8,
    mode: Mode,
    force_animated_webp: bool,
    png_mode: PngCompressionMode,
    png_optimization: PngOptimizationLevel,
) -> Result<Vec<String>, String> {
    // 统一限制质量范围到 0..=100
    let q = quality.min(100);
    let count = paths.len();
    info!(
        "compress_images start: count={}, quality={}, mode={:?}, force_animated_webp={}, png_mode={:?}, png_optimization={:?}",
        count, q, mode, force_animated_webp, png_mode, png_optimization
    );
    // 并行处理但保持顺序：记录原始索引 -> 并行处理；对每项错误记录日志并回退为原图路径
    let indexed: Vec<(usize, String)> = paths.into_iter().enumerate().collect();
    let mut v: Vec<(usize, String)> = indexed
        .into_par_iter()
        .map(|(i, p)| {
            match process_one(&p, q, mode, force_animated_webp, png_mode, png_optimization) {
                Ok(pb) => (i, pb.to_string_lossy().to_string()),
                Err(e) => {
                    error!(
                        "compress failed, fallback to original path: index={}, path={}, error={}",
                        i, p, e
                    );
                    // 回退：返回原图路径，保证顺序与长度不变
                    (i, p)
                }
            }
        })
        .collect();

    v.sort_by_key(|(i, _)| *i);
    let out: Vec<String> = v.into_iter().map(|(_, s)| s).collect();
    info!("compress_images done: count={}", out.len());
    Ok(out)
}

/// 将源文件复制到目标路径（逐一对应）。
/// 注意：此命令在后端执行文件系统复制，避免前端 FS 插件对系统临时目录的访问限制。
#[tauri::command]
pub async fn save_files(sources: Vec<String>, dests: Vec<String>) -> Result<usize, String> {
    if sources.len() != dests.len() {
        return Err(format!(
            "sources/dests length mismatch: {} vs {}",
            sources.len(),
            dests.len()
        ));
    }

    let mut ok = 0usize;
    for (src, dst) in sources.into_iter().zip(dests.into_iter()) {
        match std::fs::copy(&src, &dst) {
            Ok(_) => {
                ok += 1;
                info!("save_files: copied from {} to {}", src, dst);
            }
            Err(e) => {
                error!("save_files: copy failed from {} to {}: {}", src, dst, e);
            }
        }
    }
    Ok(ok)
}

fn process_data(
    data: Vec<u8>,
    quality: u8,
    mode: Mode,
    force_animated_webp: bool,
    png_mode: PngCompressionMode,
    png_optimization: PngOptimizationLevel,
) -> Result<PathBuf, String> {
    info!(
        "process_data start: data_len={}, quality={}, mode={:?}, force_animated_webp={}, png_mode={:?}, png_optimization={:?}",
        data.len(),
        quality,
        mode,
        force_animated_webp,
        png_mode,
        png_optimization
    );
    // 判定格式/动图属性
    let kind = detect_format_and_kind(&data)?;

    // 在应用专属临时目录创建输出文件（实际路径在 keep() 之后可被持久化）
    let tmp_dir = ensure_app_temp_dir()?;
    let mut tmp = TempFileBuilder::new()
        .prefix("yana_clipboard_")
        .suffix("")
        .tempfile_in(&tmp_dir)
        .map_err(|e| format!("tempfile_in: {}", e))?;

    match (kind, mode) {
        (DetectedKind::Static(fmt), Mode::original_format) => {
            let img = ImageReader::new(Cursor::new(&data))
                .with_guessed_format()
                .map_err(|e| format!("reader: {}", e))?
                .decode()
                .map_err(|e| format!("decode: {}", e))?;
            let out = encode_to_format(&img, fmt, quality, png_mode, png_optimization)?;
            tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
        }
        (DetectedKind::Static(_), Mode::webp) => {
            let img = ImageReader::new(Cursor::new(&data))
                .with_guessed_format()
                .map_err(|e| format!("reader: {}", e))?
                .decode()
                .map_err(|e| format!("decode: {}", e))?;
            let out = encode_webp_static(&img, quality)?;
            tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
        }
        (DetectedKind::Animated(fmt), _) => {
            if force_animated_webp {
                // 强制将动图转为 WebP：当前实现能力有限，发出警告
                warn!(
                    "animated-to-webp is enabled: data_len={}, detected_format={:?}; result may not meet expectations",
                    data.len(),
                    fmt
                );
                match fmt {
                    ImageFormat::WebP => {
                        // 已是 WebP（包含动画 WebP）：直接透传
                        tmp.write_all(&data).map_err(|e| format!("write: {}", e))?;
                    }
                    _ => {
                        // 回退：取首帧静态并编码为 WebP
                        let out = animated_to_webp(&data, quality)?;
                        tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
                    }
                }
            } else {
                // 普通动画化压缩：GIF 重编码为 GIF；动画 WebP 或未知动画保持原样
                match fmt {
                    ImageFormat::Gif => {
                        let out = reencode_gif_frames(&data, quality)?;
                        tmp.write_all(&out).map_err(|e| format!("write: {}", e))?;
                    }
                    _ => {
                        tmp.write_all(&data).map_err(|e| format!("write: {}", e))?;
                    }
                }
            }
        }
    }

    // keep() 将临时文件持久化并返回 PathBuf
    let path_buf = tmp
        .into_temp_path()
        .keep()
        .map_err(|e| format!("keep temp: {}", e))?;
    info!(
        "process_data done: data_len={}, output={}",
        data.len(),
        path_buf.display()
    );
    Ok(path_buf)
}

#[tauri::command]
pub async fn compress_image_data(
    data: Vec<u8>,
    quality: u8,
    mode: Mode,
    force_animated_webp: bool,
    png_mode: PngCompressionMode,
    png_optimization: PngOptimizationLevel,
) -> Result<String, String> {
    // 统一限制质量范围到 0..=100
    let q = quality.min(100);
    info!(
        "compress_image_data start: data_len={}, quality={}, mode={:?}, force_animated_webp={}, png_mode={:?}, png_optimization={:?}",
        data.len(),
        q,
        mode,
        force_animated_webp,
        png_mode,
        png_optimization
    );

    let path_buf = process_data(
        data,
        q,
        mode,
        force_animated_webp,
        png_mode,
        png_optimization,
    )?;
    let path_str = path_buf.to_string_lossy().to_string();

    info!("compress_image_data done: output={}", path_str);
    Ok(path_str)
}

#[tauri::command]
pub async fn save_image_data(data: Vec<u8>) -> Result<String, String> {
    info!("save_image_data start: data_len={}", data.len());

    // 在应用专属临时目录创建输出文件
    let tmp_dir = ensure_app_temp_dir()?;
    let mut tmp = TempFileBuilder::new()
        .prefix("yana_clipboard_raw_")
        .suffix("")
        .tempfile_in(&tmp_dir)
        .map_err(|e| format!("tempfile_in: {}", e))?;

    // 直接写入原始数据
    tmp.write_all(&data).map_err(|e| format!("write: {}", e))?;

    // keep() 将临时文件持久化并返回 PathBuf
    let path_buf = tmp
        .into_temp_path()
        .keep()
        .map_err(|e| format!("keep temp: {}", e))?;

    let path_str = path_buf.to_string_lossy().to_string();
    info!("save_image_data done: output={}", path_str);
    Ok(path_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use image::{ImageBuffer, ImageFormat, Rgba};
    use std::fs;

    fn write_png(path: &std::path::Path) {
        let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_fn(64, 64, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 255, 0, 255])
            }
        });
        img.save(path).expect("save png");
    }

    fn write_jpeg(path: &std::path::Path) {
        let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_fn(32, 32, |x, y| {
            let v = ((x ^ y) & 0xFF) as u8;
            Rgba([v, 128, 255 - v, 255])
        });
        let dynimg = DynamicImage::ImageRgba8(img);
        let bytes = super::encode_jpeg(&dynimg, 80).expect("encode jpeg");
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&bytes).unwrap();
    }

    fn write_webp(path: &std::path::Path) {
        let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(16, 16, Rgba([0, 0, 255, 255]));
        let dynimg = DynamicImage::ImageRgba8(img);
        let bytes = super::encode_webp_static(&dynimg, 75).expect("encode webp");
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&bytes).unwrap();
    }

    #[test]
    fn compress_folder_images_original_and_webp() {
        let dir = tempfile::tempdir().unwrap();
        let p_png = dir.path().join("a.png");
        let p_jpg = dir.path().join("b.jpg");
        let p_webp = dir.path().join("c.webp");
        write_png(&p_png);
        write_jpeg(&p_jpg);
        write_webp(&p_webp);

        let inputs: Vec<String> = vec![
            p_png.to_string_lossy().into(),
            p_jpg.to_string_lossy().into(),
            p_webp.to_string_lossy().into(),
        ];

        // 原格式压缩
        let outs_orig = block_on(super::compress_images(
            inputs.clone(),
            80,
            Mode::original_format,
            false,
            PngCompressionMode::Lossless,
            PngOptimizationLevel::Default,
        ))
        .expect("compress original");
        assert_eq!(outs_orig.len(), inputs.len());
        for (inp, out) in inputs.iter().zip(outs_orig.iter()) {
            assert!(
                std::path::Path::new(out).exists(),
                "output not exists: {}",
                out
            );
            let bytes = fs::read(out).unwrap();
            assert!(!bytes.is_empty(), "output empty: {}", out);
            let fmt = image::guess_format(&bytes).unwrap();
            // 原样模式下，输出应与输入格式一致（webp/webp，png/png，jpeg/jpeg）
            let inp_bytes = fs::read(inp).unwrap();
            let inp_fmt = image::guess_format(&inp_bytes).unwrap();
            assert_eq!(fmt, inp_fmt, "format mismatch for {} -> {}", inp, out);
        }

        // WebP 压缩
        let outs_webp = block_on(super::compress_images(
            inputs.clone(),
            75,
            Mode::webp,
            false,
            PngCompressionMode::Lossless,
            PngOptimizationLevel::Default,
        ))
        .expect("compress webp");
        assert_eq!(outs_webp.len(), inputs.len());
        for out in outs_webp.iter() {
            assert!(
                std::path::Path::new(out).exists(),
                "output not exists: {}",
                out
            );
            let bytes = fs::read(out).unwrap();
            assert!(!bytes.is_empty(), "output empty: {}", out);
            let fmt = image::guess_format(&bytes).unwrap();
            assert_eq!(fmt, ImageFormat::WebP, "expect webp, got {:?}", fmt);
        }
    }
}
