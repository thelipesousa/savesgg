//! Onde o SavesGG guarda os próprios dados (configuração, fila, preparação da nuvem).
//!
//! Instalado: `%APPDATA%\SavesGG` e `%LOCALAPPDATA%\SavesGG`.
//! Portable: se existir a pasta `dados` ao lado do executável, tudo vai para ela
//! (o app anda junto no pendrive).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Nome da pasta que liga o modo portable.
pub const PASTA_PORTATIL: &str = "dados";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pastas {
    /// Configuração e cache do manifesto.
    pub config: PathBuf,
    /// Fila de pendentes, preparação da nuvem.
    pub local: PathBuf,
    /// Rodando do pendrive (pasta `dados` ao lado do exe).
    pub portatil: bool,
}

impl Pastas {
    /// Decide as pastas a partir do executável e das pastas do Windows.
    pub fn para(exe: Option<&Path>, data: Option<PathBuf>, data_local: Option<PathBuf>) -> Pastas {
        if let Some(dados) = exe
            .and_then(Path::parent)
            .map(|d| d.join(PASTA_PORTATIL))
            .filter(|d| d.is_dir())
        {
            return Pastas {
                config: dados.clone(),
                local: dados,
                portatil: true,
            };
        }
        let local = data_local
            .unwrap_or_else(std::env::temp_dir)
            .join("SavesGG");
        Pastas {
            config: data
                .map(|d| d.join("SavesGG"))
                .unwrap_or_else(|| local.clone()),
            local,
            portatil: false,
        }
    }

    /// Pastas deste processo (decididas uma vez).
    pub fn atual() -> &'static Pastas {
        static P: OnceLock<Pastas> = OnceLock::new();
        P.get_or_init(|| {
            let exe = std::env::current_exe().ok();
            Pastas::para(exe.as_deref(), dirs::data_dir(), dirs::data_local_dir())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instalado_usa_as_pastas_do_windows() {
        let t = tempfile::tempdir().unwrap();
        let exe = t.path().join("SavesGG.exe");
        let p = Pastas::para(Some(&exe), Some("C:/R".into()), Some("C:/L".into()));
        assert_eq!(p.config, PathBuf::from("C:/R/SavesGG"));
        assert_eq!(p.local, PathBuf::from("C:/L/SavesGG"));
        assert!(!p.portatil);
    }

    #[test]
    fn pasta_dados_ao_lado_do_exe_liga_o_portable() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join(PASTA_PORTATIL)).unwrap();
        let exe = t.path().join("SavesGG.exe");
        let p = Pastas::para(Some(&exe), Some("C:/R".into()), Some("C:/L".into()));
        assert_eq!(p.config, t.path().join("dados"));
        assert_eq!(p.local, t.path().join("dados"));
        assert!(p.portatil);
    }

    #[test]
    fn arquivo_chamado_dados_nao_liga_o_portable() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join(PASTA_PORTATIL), b"x").unwrap();
        let exe = t.path().join("SavesGG.exe");
        let p = Pastas::para(Some(&exe), Some("C:/R".into()), Some("C:/L".into()));
        assert!(!p.portatil);
    }

    #[test]
    fn sem_exe_e_sem_pastas_nao_quebra() {
        let p = Pastas::para(None, None, None);
        assert!(!p.portatil);
        assert!(p.local.ends_with("SavesGG"));
        assert_eq!(p.config, p.local);
    }
}
