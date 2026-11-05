pub mod agent;
pub mod commands;
pub mod error;
pub mod logging;
pub mod model;
pub mod providers;
pub mod secrets;
pub mod skills;
pub mod state;
pub mod storage;
pub mod tools;
pub mod updater;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            logging::init(dir.join("logs"));
            logging::info(&format!("start v{}", app.package_info().version));
            let db = storage::Db::open(&dir.join("ycode.db")).map_err(|e| e.to_string())?;
            app.manage(Arc::new(state::AppState::new(db, dir)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::update_settings,
            commands::set_api_key,
            commands::delete_api_key,
            commands::set_base_url,
            commands::refresh_models,
            commands::add_manual_model,
            commands::remove_manual_model,
            commands::list_conversations,
            commands::get_conversation,
            commands::delete_conversation,
            commands::send_message,
            commands::stop_run,
            commands::respond_approval,
            commands::set_workspace,
            commands::inspect_attachment,
            commands::list_skills,
            commands::install_skill,
            commands::set_skill_enabled,
            commands::delete_skill,
            commands::validate_skill,
            commands::check_update,
            commands::install_update,
            commands::open_data_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ycode");
}
