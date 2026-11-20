//! Steam: bibliotecas em `libraryfolders.vdf`, jogos em `appmanifest_*.acf`.

use std::fs;
use std::path::{Path, PathBuf};

use keyvalues_parser::{Obj, Vdf};

use super::InstalledGame;
use crate::manifest::Store;

/// Diferença entre o SteamID64 e o SteamID3 (ID de conta).
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

fn read_vdf(path: &Path) -> Option<Vdf<'static>> {
    let text = fs::read_to_string(path).ok()?;
    keyvalues_parser::parse(&text)
        .ok()
        .map(|p| Vdf::from(p).into_owned())
}

fn get_str<'a>(obj: &'a Obj<'_>, key: &str) -> Option<&'a str> {
    obj.get(key)?.first()?.get_str()
}

/// Pastas de biblioteca (inclui a própria raiz do Steam).
fn library_dirs(steam_root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![steam_root.to_path_buf()];
    if let Some(vdf) = read_vdf(&steam_root.join("steamapps/libraryfolders.vdf"))
        && let Some(obj) = vdf.value.get_obj()
    {
        for values in obj.values() {
            for v in values {
                if let Some(path) = v.get_obj().and_then(|o| get_str(o, "path")) {
                    let p = PathBuf::from(path);
                    if !dirs.iter().any(|d| d == &p) {
                        dirs.push(p);
                    }
                }
            }
        }
    }
    dirs
}

/// SteamID3 do usuário marcado como `MostRecent` em `loginusers.vdf`.
fn most_recent_user_id3(steam_root: &Path) -> Option<String> {
    let vdf = read_vdf(&steam_root.join("config/loginusers.vdf"))?;
    let users = vdf.value.get_obj()?;
    let mut first = None;
    for (id64, values) in users.iter() {
        let Some(obj) = values.first().and_then(|v| v.get_obj()) else {
            continue;
        };
        let Ok(n) = id64.parse::<u64>() else { continue };
        let id3 = n.checked_sub(STEAM_ID64_BASE)?.to_string();
        if get_str(obj, "MostRecent") == Some("1") {
            return Some(id3);
        }
        first.get_or_insert(id3);
    }
    first
}

/// Lê os jogos instalados a partir da raiz do Steam.
pub fn steam_from_dir(steam_root: &Path) -> Vec<InstalledGame> {
    if !steam_root.join("steamapps").is_dir() {
        return Vec::new();
    }
    let user = most_recent_user_id3(steam_root);
    let mut out = Vec::new();
    for lib in library_dirs(steam_root) {
        let apps = lib.join("steamapps");
        let Ok(entries) = fs::read_dir(&apps) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Some(vdf) = read_vdf(&e.path()) else {
                continue;
            };
            let Some(obj) = vdf.value.get_obj() else {
                continue;
            };
            let (Some(appid), Some(title), Some(dir)) = (
                get_str(obj, "appid"),
                get_str(obj, "name"),
                get_str(obj, "installdir"),
            ) else {
                continue;
            };
            out.push(InstalledGame {
                name: title.to_string(),
                store: Store::Steam,
                install_dir: apps.join("common").join(dir),
                store_game_id: Some(appid.to_string()),
                store_user_id: user.clone(),
            });
        }
    }
    out
}

/// Raiz do Steam pelo registro (`HKCU\Software\Valve\Steam\SteamPath`).
#[cfg(windows)]
pub fn steam_root_from_registry() -> Option<PathBuf> {
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(r"Software\Valve\Steam")
        .ok()?;
    let path: String = key.get_value("SteamPath").ok()?;
    Some(PathBuf::from(path))
}
