fn main() {
    // Só os comandos listados aqui ficam acessíveis à interface (via capabilities).
    let commands = &[
        "scan",
        "list_destinations_cmd",
        "add_custom_folder",
        "pick_folder",
        "backup_now",
        "list_snapshots",
        "plan_restore",
        "apply_restore",
        "confirm_heuristic",
        "app_version",
        "copy_all_to",
        "get_settings",
        "set_auto_destinations",
        "set_autostart",
        "set_paused",
        "auto_status",
        "set_theme",
        "set_game_view",
        "complete_wizard",
        "cached_scan",
        "restore_many",
        "open_backup_folder",
        "set_pinned_cmd",
        "set_keep_versions",
        "set_game_keep",
        "add_manual_game",
        "remove_manual_game",
        "ignored_games",
        "open_logs_folder",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("falha no build do Tauri");
}
