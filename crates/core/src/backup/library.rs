//! `biblioteca.json`: índice de jogos e versões de um destino.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::CoreError;
use crate::protection::BackupHistory;

pub const PASTA: &str = "SaveGuard";
pub const INDICE: &str = "biblioteca.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRef {
    /// Nome dos arquivos da versão, ex. `2026-11-20_21-40-05`.
    pub stem: String,
    pub created: DateTime<Utc>,
    pub pinned: bool,
    pub content_hash: String,
    #[serde(default)]
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryIndex {
    pub version: u32,
    pub games: BTreeMap<String, Vec<SnapshotRef>>,
}

impl Default for LibraryIndex {
    fn default() -> Self {
        LibraryIndex {
            version: 1,
            games: BTreeMap::new(),
        }
    }
}

/// `<destino>\SaveGuard`.
pub fn root(dest_root: &Path) -> PathBuf {
    dest_root.join(PASTA)
}

/// Grava em `<arquivo>.tmp`, força para o disco e renomeia por cima do final.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

impl LibraryIndex {
    /// Lê o índice do destino; se não existir, devolve um índice vazio.
    pub fn load(dest_root: &Path) -> Result<Self, CoreError> {
        let path = root(dest_root).join(INDICE);
        match fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| CoreError::Formato(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_atomic(&self, dest_root: &Path) -> Result<(), CoreError> {
        let dir = root(dest_root);
        fs::create_dir_all(&dir)?;
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| CoreError::Formato(e.to_string()))?;
        write_atomic(&dir.join(INDICE), &json)
    }
}

impl BackupHistory for LibraryIndex {
    fn last_backup(&self, game_id: &str) -> Option<DateTime<Utc>> {
        self.games.get(game_id)?.iter().map(|r| r.created).max()
    }
}
