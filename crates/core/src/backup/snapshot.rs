//! Metadados de uma versão (o `.json` ao lado do `.zip`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::manifest::Store;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Caminho original com marcadores (`<winAppData>/...`).
    pub template: String,
    /// Caminho dentro do zip.
    pub zip_path: String,
    pub size: u64,
    pub blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub game_id: String,
    pub game_name: String,
    pub created: DateTime<Utc>,
    pub source_pc: String,
    pub files: Vec<FileEntry>,
    pub total_bytes: u64,
    pub content_hash: String,
    pub pinned: bool,
    pub store: Store,
    pub store_user_id: Option<String>,
    /// ID do jogo na loja (para achar a pasta de instalação no PC novo).
    #[serde(default)]
    pub store_game_id: Option<String>,
    /// Versão de segurança criada antes de uma restauração.
    #[serde(default)]
    pub seguranca: bool,
}

/// Caminho legível dentro do zip: `<winAppData>/X/a.sav` → `winAppData/X/a.sav`.
pub fn zip_path_for(template: &str) -> String {
    template
        .chars()
        .filter(|c| !matches!(c, '<' | '>'))
        .map(|c| if c == ':' { '_' } else { c })
        .collect::<String>()
        .trim_start_matches('/')
        .to_string()
}
