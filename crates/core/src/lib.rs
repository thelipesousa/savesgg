//! Motor do SaveGuard: varredura, prioridade, backup e restauração de saves.

pub mod manifest;
pub mod paths;
pub mod stores;

/// Erros do motor.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("erro de arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("formato inválido: {0}")]
    Formato(String),
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
