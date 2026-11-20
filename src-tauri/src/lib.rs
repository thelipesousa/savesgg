mod auto;
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
            auto::spawn(app.handle().clone());
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

/// Chamado quando o estado do backup automático muda (a bandeja se atualiza aqui).
pub(crate) fn on_status_changed(_app: &tauri::AppHandle, _status: &auto::AutoStatusView) {}

/// Avisa o usuário (notificação do Windows).
pub(crate) fn notify_user(_app: &tauri::AppHandle, title: &str, body: &str) {
    eprintln!("{title}: {body}");
}
