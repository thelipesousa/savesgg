mod commands;
mod manifest_source;
mod state;
mod views;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let mut bundled = Vec::new();
            if let Ok(dir) = app.path().resource_dir() {
                bundled.push(dir.join("resources").join("manifest.yaml"));
            }
            #[cfg(debug_assertions)]
            bundled.push(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/manifest.yaml"),
            );
            app.manage(state::AppState::new(bundled));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::list_destinations_cmd,
            commands::add_custom_folder,
            commands::pick_folder,
            commands::backup_now,
            commands::list_snapshots,
            commands::plan_restore,
            commands::apply_restore,
            commands::confirm_heuristic,
            commands::app_version,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o SaveGuard");
}
