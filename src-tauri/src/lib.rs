// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod gallery;
mod image_hosts;
mod process;
mod s3;
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
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_os::init())
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
            process::compress_image_data,
            process::save_image_data,
            process::save_files,
            process::clean_app_temp_dir,
            process::get_file_sizes,
            settings::load_settings,
            settings::save_settings,
            settings::open_log_dir,
            image_hosts::list_image_host_plugins,
            image_hosts::load_image_host_settings,
            image_hosts::save_image_host_settings,
            image_hosts::add_image_host_plugin,
            upload::upload_image,
            s3::s3_upload,
            s3::s3_delete,
            gallery::gallery_insert_item,
            gallery::gallery_delete_item,
            gallery::gallery_query_items,
            gallery::gallery_list_hosts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
