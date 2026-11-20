//! Jogos instalados nas lojas (Steam, Epic, GOG, Xbox).

use std::path::PathBuf;

use crate::manifest::Store;

mod epic;
#[cfg(windows)]
mod gog;
mod steam;
mod xbox;

pub use epic::epic_from_dir;
#[cfg(windows)]
pub use gog::gog_from_registry;
pub use steam::steam_from_dir;
pub use xbox::xbox_from_packages;

/// Um jogo instalado por alguma loja.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledGame {
    pub name: String,
    pub store: Store,
    pub install_dir: PathBuf,
    pub store_game_id: Option<String>,
    pub store_user_id: Option<String>,
}

/// Detecta os jogos de todas as lojas encontradas neste PC.
pub fn detect_all() -> Vec<InstalledGame> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Some(root) = steam::steam_root_from_registry() {
            out.extend(steam_from_dir(&root));
        }
        out.extend(gog_from_registry());
    }
    let program_data = std::env::var_os("PROGRAMDATA")
        .map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
    out.extend(epic_from_dir(
        &program_data.join(r"Epic\EpicGamesLauncher\Data\Manifests"),
    ));
    if let Some(local) = dirs::data_local_dir() {
        out.extend(xbox_from_packages(&local.join("Packages")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn vdf_path(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "\\\\")
    }

    /// Monta uma instalação Steam falsa com duas bibliotecas.
    fn steam_fake(root: &Path) -> PathBuf {
        let steam = root.join("Steam");
        let lib2 = root.join("SteamLibrary");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::create_dir_all(steam.join("config")).unwrap();
        fs::create_dir_all(lib2.join("steamapps/common/ELDEN RING")).unwrap();
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t}}\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"1245620\"\t\t\"123\"\n\t\t}}\n\t}}\n}}\n",
                vdf_path(&steam),
                vdf_path(&lib2)
            ),
        )
        .unwrap();
        fs::write(
            lib2.join("steamapps/appmanifest_1245620.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1245620\"\n\t\"name\"\t\t\"ELDEN RING\"\n\t\"installdir\"\t\t\"ELDEN RING\"\n}\n",
        )
        .unwrap();
        fs::write(
            steam.join("config/loginusers.vdf"),
            "\"users\"\n{\n\t\"76561197960265729\"\n\t{\n\t\t\"AccountName\"\t\t\"velho\"\n\t\t\"MostRecent\"\t\t\"0\"\n\t}\n\t\"76561198025000000\"\n\t{\n\t\t\"AccountName\"\t\t\"atual\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t}\n}\n",
        )
        .unwrap();
        steam
    }

    #[test]
    fn steam_le_bibliotecas() {
        let t = tempfile::tempdir().unwrap();
        let games = steam_from_dir(&steam_fake(t.path()));
        assert_eq!(games.len(), 1);
        let er = &games[0];
        assert_eq!(er.name, "ELDEN RING");
        assert_eq!(er.store, Store::Steam);
        assert!(
            er.install_dir.ends_with("steamapps/common/ELDEN RING"),
            "{:?}",
            er.install_dir
        );
        assert_eq!(er.store_game_id.as_deref(), Some("1245620"));
    }

    #[test]
    fn steam_user_id3() {
        let t = tempfile::tempdir().unwrap();
        let games = steam_from_dir(&steam_fake(t.path()));
        assert_eq!(games[0].store_user_id.as_deref(), Some("64734272"));
    }

    #[test]
    fn epic_le_item() {
        let t = tempfile::tempdir().unwrap();
        fs::write(
            t.path().join("ABC.item"),
            r#"{"DisplayName": "Hades II", "InstallLocation": "D:\\Epic\\HadesII", "AppName": "Kiwi"}"#,
        )
        .unwrap();
        fs::write(t.path().join("lixo.txt"), "nada").unwrap();
        let games = epic_from_dir(t.path());
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Hades II");
        assert_eq!(games[0].install_dir, PathBuf::from(r"D:\Epic\HadesII"));
        assert_eq!(games[0].store_game_id.as_deref(), Some("Kiwi"));
    }

    #[test]
    fn pasta_inexistente_vazia() {
        assert!(steam_from_dir(Path::new("Z:/nada")).is_empty());
        assert!(epic_from_dir(Path::new("Z:/nada")).is_empty());
        assert!(xbox_from_packages(Path::new("Z:/nada")).is_empty());
    }

    #[test]
    fn xbox_wgs() {
        let t = tempfile::tempdir().unwrap();
        let wgs = t
            .path()
            .join("Microsoft.Forza_8wekyb3d8bbwe/SystemAppData/wgs");
        fs::create_dir_all(&wgs).unwrap();
        fs::write(wgs.join("arquivo"), "x").unwrap();
        fs::create_dir_all(t.path().join("Outro.App_123/SystemAppData")).unwrap();
        let games = xbox_from_packages(t.path());
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Microsoft.Forza");
        assert_eq!(games[0].store, Store::Xbox);
        assert_eq!(games[0].install_dir, wgs);
    }
}

#[cfg(test)]
mod pc_real {
    /// Mostra o que foi detectado neste PC: `cargo test -- --ignored detecta_pc_real --nocapture`.
    #[test]
    #[ignore]
    fn detecta_pc_real() {
        let games = super::detect_all();
        for g in &games {
            eprintln!("{:?} | {} | {}", g.store, g.name, g.install_dir.display());
        }
        eprintln!("total: {}", games.len());
    }
}
