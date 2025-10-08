// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod process;
mod settings;
mod upload;

use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let is_dev = cfg!(debug_assertions);
    let log_level = if is_dev {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let log_targets = if is_dev {
        vec![
            Target::new(TargetKind::LogDir { file_name: None }),
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::Webview),
        ]
    } else {
        vec![Target::new(TargetKind::LogDir { file_name: None })]
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        // 日志：根据环境选择输出目标与日志级别，开发环境输出到控制台/前端，生产仅写文件
        .plugin(
            tauri_plugin_log::Builder::new()
                .rotation_strategy(RotationStrategy::KeepSome(5))
                .max_file_size(128u128 * 1024 * 1024)
                .targets(log_targets)
                .level(log_level)
                .format(move |out, message, record| {
                    out.finish(format_args!(
                        "{} [{}] [{}] {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
                        record.level(),
                        record.target(),
                        message
                    ))
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            process::compress_images,
            process::save_files,
            settings::load_settings,
            settings::save_settings,
            settings::open_log_dir,
            upload::upload_image
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
