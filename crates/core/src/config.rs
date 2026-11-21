//! Configuração do app em `%APPDATA%\SaveGuard\config.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::backup_write_atomic;
pub use crate::scan::ManualGame;

/// Temas da interface (o primeiro é o padrão).
pub const TEMAS: [&str; 5] = ["gamer", "pastel", "win11", "cyberpunk", "console"];
/// Exibição da lista de jogos (o primeiro é o padrão).
pub const EXIBICOES: [&str; 2] = ["lista", "grade"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// IDs dos destinos do backup automático (plano 2).
    pub auto_destinations: Vec<String>,
    pub custom_folders: Vec<PathBuf>,
    pub keep_versions: usize,
    pub per_game_keep: BTreeMap<String, usize>,
    pub manual_games: Vec<ManualGame>,
    pub ignored_games: Vec<String>,
    /// Possíveis saves (heurística) que o usuário confirmou.
    pub confirmed_games: Vec<String>,
    pub theme: String,
    /// Iniciar com o Windows (minimizado na bandeja).
    pub autostart: bool,
    /// Backup automático pausado pelo usuário.
    pub auto_paused: bool,
    /// "lista" ou "grade" de capas.
    pub game_view: String,
    /// Assistente da primeira execução concluído (ou pulado).
    pub wizard_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: 1,
            auto_destinations: Vec::new(),
            custom_folders: Vec::new(),
            keep_versions: 10,
            per_game_keep: BTreeMap::new(),
            manual_games: Vec::new(),
            ignored_games: Vec::new(),
            confirmed_games: Vec::new(),
            theme: "gamer".into(),
            autostart: true,
            auto_paused: false,
            game_view: EXIBICOES[0].into(),
            wizard_done: false,
        }
    }
}

impl Config {
    /// Caminho padrão: `%APPDATA%\SaveGuard\config.json`.
    pub fn default_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_default()
            .join("SaveGuard")
            .join("config.json")
    }

    /// Lê a configuração. Arquivo ausente = padrão; corrompido = padrão, guardando o
    /// original como `config.json.corrompido` para não perder nada.
    pub fn load_or_default(path: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Config::default();
        };
        // Ignora o BOM que o Bloco de Notas e o PowerShell 5.1 colocam no início.
        match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
            Ok(c) => c,
            Err(_) => {
                let mut bad = path.as_os_str().to_os_string();
                bad.push(".corrompido");
                let _ = std::fs::rename(path, PathBuf::from(bad));
                Config::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| CoreError::Formato(e.to_string()))?;
        backup_write_atomic(path, &json)
    }

    /// Tema salvo, ou o padrão se o valor não for conhecido (config editada à mão).
    pub fn theme_or_default(&self) -> &str {
        TEMAS
            .iter()
            .find(|t| **t == self.theme)
            .copied()
            .unwrap_or(TEMAS[0])
    }

    /// Exibição salva, ou o padrão se o valor não for conhecido.
    pub fn game_view_or_default(&self) -> &str {
        EXIBICOES
            .iter()
            .find(|t| **t == self.game_view)
            .copied()
            .unwrap_or(EXIBICOES[0])
    }

    /// Quantas versões manter para um jogo.
    pub fn keep_for(&self, game_id: &str) -> usize {
        self.per_game_keep
            .get(game_id)
            .copied()
            .unwrap_or(self.keep_versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn config_padrao() {
        let t = tempfile::tempdir().unwrap();
        let c = Config::load_or_default(&t.path().join("config.json"));
        assert_eq!(c.version, 1);
        assert_eq!(c.keep_versions, 10);
        assert_eq!(c.theme, "gamer");
        assert!(c.auto_destinations.is_empty());
    }

    #[test]
    fn config_ida_e_volta() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("sub/config.json");
        let mut c = Config::default();
        c.auto_destinations
            .push("sync:c:/users/ana/onedrive".into());
        c.per_game_keep.insert("elden-ring".into(), 3);
        c.ignored_games.push("ebwebview".into());
        c.manual_games.push(ManualGame {
            name: "Meu Jogo".into(),
            save_dir: "D:/Jogos/Meu".into(),
            exe: None,
        });
        c.save(&p).unwrap();
        assert_eq!(Config::load_or_default(&p), c);
    }

    #[test]
    fn config_corrompida_usa_padrao_e_guarda_copia() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("config.json");
        fs::write(&p, "{ isso não é json").unwrap();
        let c = Config::load_or_default(&p);
        assert_eq!(c, Config::default());
        assert!(t.path().join("config.json.corrompido").is_file());
    }

    #[test]
    fn config_antiga_ganha_padroes() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("config.json");
        fs::write(&p, r#"{"version":1,"keep_versions":7,"theme":"gamer"}"#).unwrap();
        let c = Config::load_or_default(&p);
        assert_eq!(c.keep_versions, 7);
        assert!(c.autostart);
        assert!(!c.auto_paused);
    }

    #[test]
    fn config_com_bom_e_lida() {
        // Bloco de Notas e PowerShell 5.1 gravam UTF-8 com BOM.
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("config.json");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(br#"{"version":1,"keep_versions":4,"theme":"gamer"}"#);
        fs::write(&p, bytes).unwrap();
        let c = Config::load_or_default(&p);
        assert_eq!(c.keep_versions, 4);
        assert!(!t.path().join("config.json.corrompido").exists());
    }

    #[test]
    fn tema_invalido_usa_padrao() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("config.json");
        fs::write(&p, r#"{"version":1,"theme":"azul","game_view":"mosaico"}"#).unwrap();
        let c = Config::load_or_default(&p);
        assert_eq!(c.theme_or_default(), "gamer");
        assert_eq!(c.game_view_or_default(), "lista");
        let ok = Config {
            theme: "pastel".into(),
            game_view: "grade".into(),
            ..Default::default()
        };
        assert_eq!(ok.theme_or_default(), "pastel");
        assert_eq!(ok.game_view_or_default(), "grade");
    }

    #[test]
    fn config_antiga_mostra_assistente() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("config.json");
        fs::write(&p, r#"{"version":1}"#).unwrap();
        let c = Config::load_or_default(&p);
        assert!(!c.wizard_done);
        assert_eq!(c.game_view, "lista");
    }

    #[test]
    fn keep_por_jogo() {
        let mut c = Config::default();
        c.per_game_keep.insert("hades-ii".into(), 3);
        assert_eq!(c.keep_for("hades-ii"), 3);
        assert_eq!(c.keep_for("outro"), 10);
    }
}
