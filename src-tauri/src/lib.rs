mod auto;
mod commands;
mod drives;
mod manifest_source;
mod state;
mod tray;
mod views;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Log em %LOCALAPPDATA%\com.savesgg.app\logs (1 MB por arquivo, guarda 5).
        // Nunca registra conteúdo de saves nem tokens.
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("savesgg".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .max_file_size(1_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        // Uma instância só: abrir de novo traz a janela que já existe.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimizado"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // Atualização assinada (chave pública em tauri.conf.json) e reinício depois dela.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .on_window_event(|window, event| {
            // Fechar a janela só esconde: o app segue protegendo na bandeja.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
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
            log::info!("SavesGG {} iniciando", env!("CARGO_PKG_VERSION"));
            app.manage(state::AppState::new(bundled));
            sync_autostart(app.handle());
            tray::build(app.handle())?;
            if !std::env::args().any(|a| a == "--minimizado") {
                tray::show_main(app.handle());
            }
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
            commands::copy_all_to,
            commands::get_settings,
            commands::set_auto_destinations,
            commands::set_autostart,
            commands::set_paused,
            commands::auto_status,
            commands::set_theme,
            commands::set_game_view,
            commands::complete_wizard,
            commands::cached_scan,
            commands::restore_many,
            commands::open_backup_folder,
            commands::set_pinned_cmd,
            commands::set_keep_versions,
            commands::set_game_keep,
            commands::add_manual_game,
            commands::remove_manual_game,
            commands::ignored_games,
            commands::open_logs_folder,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o SavesGG");
}

/// Chamado quando o estado do backup automático muda (a bandeja se atualiza aqui).
pub(crate) fn on_status_changed(app: &tauri::AppHandle, status: &auto::AutoStatusView) {
    tray::update(app, status);
}

/// Liga/desliga o início com o Windows conforme a configuração. Só no app instalado:
/// em desenvolvimento registraria o executável de teste no Windows.
pub(crate) fn sync_autostart(app: &tauri::AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    use tauri_plugin_autostart::ManagerExt;
    let want = app.state::<state::AppState>().config().autostart;
    let m = app.autolaunch();
    let r = match (want, m.is_enabled().unwrap_or(false)) {
        (true, false) => m.enable(),
        (false, true) => m.disable(),
        _ => Ok(()),
    };
    if let Err(e) = r {
        log::warn!("início com o Windows: {e}");
    }
}

/// Avisa o usuário (notificação do Windows).
pub(crate) fn notify_user(app: &tauri::AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("notificação ({title}): {e}");
    }
}
