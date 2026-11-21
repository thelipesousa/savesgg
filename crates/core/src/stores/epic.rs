//! Epic Games: manifestos `.item` (JSON) em `ProgramData\Epic\EpicGamesLauncher\Data\Manifests`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::InstalledGame;
use crate::manifest::Store;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Item {
    display_name: String,
    install_location: String,
    app_name: Option<String>,
}

pub fn epic_from_dir(manifests_dir: &Path) -> Vec<InstalledGame> {
    let Ok(entries) = fs::read_dir(manifests_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("item"))
        })
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|text| serde_json::from_str::<Item>(&text).ok())
        .map(|i| InstalledGame {
            name: i.display_name,
            store: Store::Epic,
            install_dir: PathBuf::from(i.install_location),
            store_game_id: i.app_name,
            store_user_id: None,
            store_root: None,
        })
        .collect()
}
