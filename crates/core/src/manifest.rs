//! Manifesto de locais de save do Ludusavi (dados da PCGamingWiki, licença MIT).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// De onde o jogo veio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    Steam,
    Epic,
    Gog,
    Xbox,
    Emulador,
    SemLoja,
}

impl Store {
    /// Nome usado no campo `store` do manifesto.
    fn manifest_name(self) -> Option<&'static str> {
        match self {
            Store::Steam => Some("steam"),
            Store::Epic => Some("epic"),
            Store::Gog => Some("gog"),
            Store::Xbox => Some("microsoft"),
            Store::Emulador | Store::SemLoja => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub games: BTreeMap<String, GameEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct GameEntry {
    pub files: BTreeMap<String, FileRule>,
    pub install_dir: Vec<String>,
    pub steam_id: Option<u32>,
    pub gog_id: Option<u64>,
    pub cloud: CloudFlags,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FileRule {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub when: Vec<When>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct When {
    pub os: Option<String>,
    pub store: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudFlags {
    #[serde(default)]
    pub steam: bool,
    #[serde(default)]
    pub epic: bool,
    #[serde(default)]
    pub gog: bool,
    #[serde(default)]
    pub origin: bool,
    #[serde(default)]
    pub uplay: bool,
}

#[derive(Deserialize)]
struct RawId<T> {
    id: Option<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    #[serde(default)]
    files: BTreeMap<String, FileRule>,
    #[serde(default)]
    install_dir: BTreeMap<String, serde_yaml::Value>,
    steam: Option<RawId<u32>>,
    gog: Option<RawId<u64>>,
    #[serde(default)]
    cloud: CloudFlags,
}

/// Lê o YAML do manifesto. Campos desconhecidos são ignorados.
pub fn parse(yaml: &str) -> Result<Manifest, CoreError> {
    let raw: BTreeMap<String, RawEntry> =
        serde_yaml::from_str(yaml).map_err(|e| CoreError::Formato(e.to_string()))?;
    let games = raw
        .into_iter()
        .map(|(name, r)| {
            let entry = GameEntry {
                files: r.files,
                install_dir: r.install_dir.into_keys().collect(),
                steam_id: r.steam.and_then(|s| s.id),
                gog_id: r.gog.and_then(|g| g.id),
                cloud: r.cloud,
            };
            (name, entry)
        })
        .collect();
    Ok(Manifest { games })
}

impl GameEntry {
    /// Padrões de save (tag `save`) válidos no Windows para a loja informada.
    pub fn save_paths_for(&self, store: Option<Store>) -> Vec<&str> {
        let store_name = store.and_then(Store::manifest_name);
        self.files
            .iter()
            .filter(|(_, rule)| rule.tags.iter().any(|t| t == "save"))
            .filter(|(_, rule)| {
                rule.when.is_empty()
                    || rule.when.iter().any(|w| {
                        let os_ok = w.os.as_deref().is_none_or(|o| o == "windows");
                        let store_ok = match w.store.as_deref() {
                            None => true,
                            Some(s) => Some(s) == store_name,
                        };
                        os_ok && store_ok
                    })
            })
            .map(|(path, _)| path.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mini() -> Manifest {
        parse(include_str!("../tests/fixtures/manifest-mini.yaml")).unwrap()
    }

    #[test]
    fn parse_mini() {
        let m = mini();
        assert_eq!(m.games.len(), 5);
        let er = &m.games["Elden Ring"];
        assert_eq!(er.steam_id, Some(1245620));
        assert!(er.cloud.steam);
        assert_eq!(er.install_dir, vec!["ELDEN RING".to_string()]);
        assert_eq!(m.games["Hollow Knight"].gog_id, Some(1308320804));
    }

    #[test]
    fn filtra_linux_e_config() {
        let m = mini();
        let p = m.games["Hollow Knight"].save_paths_for(Some(Store::Steam));
        assert!(p.contains(&"<home>/AppData/LocalLow/Team Cherry/Hollow Knight/*.dat"));
        assert!(!p.iter().any(|x| x.contains("xdgConfig")), "{p:?}");
        assert!(!p.iter().any(|x| x.ends_with("Config.ini")), "{p:?}");
    }

    #[test]
    fn filtra_por_loja() {
        let m = mini();
        let hk = &m.games["Hollow Knight"];
        let steam = hk.save_paths_for(Some(Store::Steam));
        assert!(!steam.iter().any(|x| x.contains("wgs")), "{steam:?}");
        let xbox = hk.save_paths_for(Some(Store::Xbox));
        assert!(xbox.iter().any(|x| x.contains("wgs")), "{xbox:?}");
    }

    #[test]
    fn ignora_campos_desconhecidos() {
        let y = "Jogo:\n  registry:\n    HKEY_CURRENT_USER/X: {}\n  qualquer: 1\n  files:\n    \"<base>/s.sav\":\n      tags: [save]\n";
        let m = parse(y).unwrap();
        assert_eq!(m.games["Jogo"].save_paths_for(None), vec!["<base>/s.sav"]);
    }

    /// Lê o manifesto completo: `SAVEGUARD_MANIFEST=<arquivo> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn parse_manifesto_real() {
        let path = std::env::var("SAVEGUARD_MANIFEST").expect("defina SAVEGUARD_MANIFEST");
        let t = std::time::Instant::now();
        let m = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        eprintln!("{} jogos em {:?}", m.games.len(), t.elapsed());
        assert!(m.games.len() > 10_000);
    }

    #[test]
    fn yaml_invalido_e_erro() {
        assert!(parse("isso: [nao fecha").is_err());
    }
}
