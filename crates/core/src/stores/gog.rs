//! GOG: jogos registrados em `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games`.

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

use super::InstalledGame;
use crate::manifest::Store;

pub fn gog_from_registry() -> Vec<InstalledGame> {
    let Ok(games) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games")
    else {
        return Vec::new();
    };
    games
        .enum_keys()
        .flatten()
        .filter_map(|id| {
            let k = games.open_subkey(&id).ok()?;
            let name: String = k.get_value("gameName").ok()?;
            let path: String = k.get_value("path").ok()?;
            Some(InstalledGame {
                name,
                store: Store::Gog,
                install_dir: PathBuf::from(path),
                store_game_id: Some(id),
                store_user_id: None,
                store_root: None,
            })
        })
        .collect()
}
