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
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("falha no build do Tauri");
}
