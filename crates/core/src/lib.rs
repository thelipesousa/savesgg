//! Motor do SaveGuard: varredura, prioridade, backup e restauração de saves.

pub mod backup;
pub mod config;
pub mod destinations;
pub mod emulators;
mod heuristics;
pub mod manifest;
pub mod paths;
pub mod pending;
pub mod protection;
pub mod restore;
pub mod scan;
pub mod stores;
pub mod sync_folders;
pub mod watch;

pub(crate) use backup::write_atomic as backup_write_atomic;

/// Erros do motor.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("erro de arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("formato inválido: {0}")]
    Formato(String),
    #[error("arquivo em uso por outro programa (o jogo está aberto?): {0}")]
    ArquivoTravado(std::path::PathBuf),
    #[error("o jogo está gravando este arquivo agora; tente de novo com o jogo fechado: {0}")]
    ArquivoMudou(std::path::PathBuf),
    #[error("destino indisponível: {0}")]
    DestinoIndisponivel(std::path::PathBuf),
    #[error("caminho inseguro no backup: {0}")]
    CaminhoInseguro(String),
}

/// Versão do motor.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_nao_vazia() {
        assert!(!version().is_empty());
    }
}
