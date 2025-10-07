// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod process;
mod settings;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // 日志：输出到多平台日志目录，最大文件 128MB，开启轮转（保留最近 5 个），也输出到控制台与前端 Webview
        .plugin(
            tauri_plugin_log::Builder::new()
                .rotation_strategy(RotationStrategy::KeepSome(5))
                .max_file_size(128u128 * 1024 * 1024)
                .targets([
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            process::compress_images,
            process::save_files,
            settings::load_settings,
            settings::save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
