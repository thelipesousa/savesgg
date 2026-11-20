//! Pastas sincronizadas por OneDrive, Google Drive e Dropbox.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncProvider {
    OneDrive,
    GoogleDrive,
    Dropbox,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncRoot {
    pub provider: SyncProvider,
    pub path: PathBuf,
}

/// Lê o `info.json` do Dropbox (contas pessoal e empresarial).
pub fn dropbox_from_info(info: &Path) -> Vec<SyncRoot> {
    let Some(v) = std::fs::read_to_string(info)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return Vec::new();
    };
    ["personal", "business"]
        .iter()
        .filter_map(|k| v.get(k)?.get("path")?.as_str())
        .map(|p| SyncRoot {
            provider: SyncProvider::Dropbox,
            path: PathBuf::from(p),
        })
        .collect()
}

/// Detecta as pastas sincronizadas neste PC.
#[cfg(windows)]
pub fn detect_sync_roots() -> Vec<SyncRoot> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let mut out: Vec<SyncRoot> = Vec::new();
    let mut push = |r: SyncRoot| {
        if r.path.is_dir() && !out.contains(&r) {
            out.push(r);
        }
    };

    // OneDrive: uma subchave por conta (Personal, Business1...).
    if let Ok(accounts) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\OneDrive\Accounts")
    {
        for name in accounts.enum_keys().flatten() {
            if let Ok(folder) = accounts
                .open_subkey(&name)
                .and_then(|k| k.get_value::<String, _>("UserFolder"))
            {
                push(SyncRoot {
                    provider: SyncProvider::OneDrive,
                    path: PathBuf::from(folder),
                });
            }
        }
    }

    // Dropbox.
    for base in [dirs::data_dir(), dirs::data_local_dir()]
        .into_iter()
        .flatten()
    {
        for r in dropbox_from_info(&base.join("Dropbox").join("info.json")) {
            push(r);
        }
    }

    // Google Drive para computador: unidade virtual com "Meu Drive" / "My Drive".
    for letter in b'D'..=b'Z' {
        for folder in ["My Drive", "Meu Drive"] {
            let p = PathBuf::from(format!("{}:\\{folder}", letter as char));
            push(SyncRoot {
                provider: SyncProvider::GoogleDrive,
                path: p,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn dropbox_le_info_json() {
        let t = tempfile::tempdir().unwrap();
        let info = t.path().join("info.json");
        fs::write(
            &info,
            r#"{"personal": {"path": "D:\\Dropbox", "host": 1}, "business": {"path": "D:\\Dropbox (Empresa)"}}"#,
        )
        .unwrap();
        let roots = dropbox_from_info(&info);
        assert_eq!(
            roots,
            vec![
                SyncRoot {
                    provider: SyncProvider::Dropbox,
                    path: PathBuf::from(r"D:\Dropbox")
                },
                SyncRoot {
                    provider: SyncProvider::Dropbox,
                    path: PathBuf::from(r"D:\Dropbox (Empresa)")
                },
            ]
        );
    }

    #[test]
    fn dropbox_sem_arquivo_vazio() {
        assert!(dropbox_from_info(std::path::Path::new("Z:/nada/info.json")).is_empty());
    }
}

#[cfg(all(test, windows))]
mod pc_real {
    /// `cargo test -- --ignored detecta_sync_real --nocapture`
    #[test]
    #[ignore]
    fn detecta_sync_real() {
        eprintln!("{:?}", super::detect_sync_roots());
    }
}
