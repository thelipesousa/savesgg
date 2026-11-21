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
