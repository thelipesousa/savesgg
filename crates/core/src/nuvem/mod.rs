//! Contas de nuvem (Google Drive): a conta vira um destino com uma pasta local de
//! preparação no formato de sempre, e o enviador sincroniza essa pasta com a nuvem.

pub mod nomes;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Serviços de nuvem com login dentro do app (o OneDrive entra depois, issue #27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvedorNuvem {
    Gdrive,
}

impl ProvedorNuvem {
    pub fn chave(self) -> &'static str {
        match self {
            ProvedorNuvem::Gdrive => "gdrive",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            ProvedorNuvem::Gdrive => "Google Drive",
        }
    }
}

/// Conta conectada (sem tokens: eles ficam no Gerenciador de Credenciais).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContaNuvem {
    pub provedor: ProvedorNuvem,
    /// Id da conta no provedor (`sub` do Google).
    pub id: String,
    pub email: String,
    pub nome: String,
}

impl ContaNuvem {
    /// `nuvem:gdrive:<id>`.
    pub fn destino_id(&self) -> String {
        format!("nuvem:{}:{}", self.provedor.chave(), self.id)
    }
}

/// Pasta local de preparação de uma conta: `<base>/nuvem/<blake3(id)[..16]>`.
pub fn pasta_preparacao(base: &Path, destino_id: &str) -> PathBuf {
    base.join("nuvem")
        .join(&blake3::hash(destino_id.as_bytes()).to_hex()[..16])
}

#[cfg(test)]
pub mod falso;
pub mod gdrive;
pub mod http;
pub mod sync;

/// Item de uma pasta na nuvem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRemoto {
    pub nome: String,
    pub pasta: bool,
    pub tamanho: u64,
}

/// Erros da nuvem. As mensagens vão para a tela: nunca levam token.
#[derive(Debug, thiserror::Error)]
pub enum ErroNuvem {
    #[error("sem conexão com a nuvem: {0}")]
    Rede(String),
    #[error("entre de novo na conta")]
    Login,
    #[error("a nuvem recusou: {0}")]
    Servidor(String),
    #[error("{0}")]
    Local(String),
}

/// Operações de um serviço de nuvem. Os caminhos são relativos à raiz remota do
/// app, com `/`, por exemplo `SavesGG/elden-ring/2026-11-20_21-40-05.zip`.
pub trait Provedor: Send + Sync {
    fn listar(&self, pasta: &str) -> Result<Vec<ItemRemoto>, ErroNuvem>;
    fn enviar(&self, caminho: &str, origem: &Path) -> Result<(), ErroNuvem>;
    fn baixar(&self, caminho: &str, destino: &Path) -> Result<(), ErroNuvem>;
    fn apagar(&self, caminho: &str) -> Result<(), ErroNuvem>;
}
