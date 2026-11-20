//! Motor do SaveGuard: varredura, prioridade, backup e restauração de saves.

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
