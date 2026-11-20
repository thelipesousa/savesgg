//! Xbox / Game Pass: saves em `%LOCALAPPDATA%\Packages\<pacote>\SystemAppData\wgs`.
//! Os nomes dentro de `wgs` são embaralhados; o backup é bruto.

use std::fs;
use std::path::Path;

use super::InstalledGame;
use crate::manifest::Store;

pub fn xbox_from_packages(packages_dir: &Path) -> Vec<InstalledGame> {
    let Ok(entries) = fs::read_dir(packages_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let wgs = e.path().join("SystemAppData").join("wgs");
            let has_content = fs::read_dir(&wgs).ok()?.next().is_some();
            if !has_content {
                return None;
            }
            let pkg = e.file_name().to_string_lossy().into_owned();
            let name = pkg.split('_').next().unwrap_or(&pkg).to_string();
            Some(InstalledGame {
                name,
                store: Store::Xbox,
                install_dir: wgs,
                store_game_id: Some(pkg),
                store_user_id: None,
            })
        })
        .collect()
}
