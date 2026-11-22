//! Nuvem falsa em memória, para testar o enviador sem internet.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use super::{ErroNuvem, ItemRemoto, Provedor};

#[derive(Default)]
pub struct ProvedorFalso {
    arquivos: Mutex<BTreeMap<String, Vec<u8>>>,
    /// Quantas operações ainda funcionam antes de "cair a internet".
    restantes: Mutex<Option<usize>>,
    login: Mutex<bool>,
    envios: Mutex<usize>,
    baixados: Mutex<usize>,
}

impl ProvedorFalso {
    pub fn por(&self, caminho: &str, dados: &[u8]) {
        self.arquivos
            .lock()
            .unwrap()
            .insert(caminho.to_string(), dados.to_vec());
    }
    pub fn tem(&self, caminho: &str) -> bool {
        self.arquivos.lock().unwrap().contains_key(caminho)
    }
    pub fn ler(&self, caminho: &str) -> Vec<u8> {
        self.arquivos.lock().unwrap()[caminho].clone()
    }
    pub fn falhar_apos(&self, n: usize) {
        *self.restantes.lock().unwrap() = Some(n);
    }
    pub fn sem_falha(&self) {
        *self.restantes.lock().unwrap() = None;
    }
    pub fn sempre_login(&self) {
        *self.login.lock().unwrap() = true;
    }
    pub fn envios(&self) -> usize {
        *self.envios.lock().unwrap()
    }
    pub fn baixados(&self) -> usize {
        *self.baixados.lock().unwrap()
    }

    fn operar(&self) -> Result<(), ErroNuvem> {
        if *self.login.lock().unwrap() {
            return Err(ErroNuvem::Login);
        }
        let mut r = self.restantes.lock().unwrap();
        match r.as_mut() {
            Some(0) => Err(ErroNuvem::Rede("internet caiu (falso)".into())),
            Some(n) => {
                *n -= 1;
                Ok(())
            }
            None => Ok(()),
        }
    }
}

impl Provedor for ProvedorFalso {
    fn listar(&self, pasta: &str) -> Result<Vec<ItemRemoto>, ErroNuvem> {
        self.operar()?;
        let prefixo = format!("{}/", pasta.trim_end_matches('/'));
        let mut vistos = BTreeMap::new();
        for (k, v) in self.arquivos.lock().unwrap().iter() {
            let Some(resto) = k.strip_prefix(&prefixo) else {
                continue;
            };
            match resto.split_once('/') {
                Some((dir, _)) => {
                    vistos.entry(dir.to_string()).or_insert(ItemRemoto {
                        nome: dir.to_string(),
                        pasta: true,
                        tamanho: 0,
                    });
                }
                None => {
                    vistos.insert(
                        resto.to_string(),
                        ItemRemoto {
                            nome: resto.to_string(),
                            pasta: false,
                            tamanho: v.len() as u64,
                        },
                    );
                }
            }
        }
        Ok(vistos.into_values().collect())
    }

    fn enviar(&self, caminho: &str, origem: &Path) -> Result<(), ErroNuvem> {
        self.operar()?;
        let dados = std::fs::read(origem).map_err(|e| ErroNuvem::Local(e.to_string()))?;
        self.por(caminho, &dados);
        *self.envios.lock().unwrap() += 1;
        Ok(())
    }

    fn baixar(&self, caminho: &str, destino: &Path) -> Result<(), ErroNuvem> {
        self.operar()?;
        let dados = self
            .arquivos
            .lock()
            .unwrap()
            .get(caminho)
            .cloned()
            .ok_or_else(|| ErroNuvem::Servidor(format!("não existe: {caminho}")))?;
        if let Some(d) = destino.parent() {
            std::fs::create_dir_all(d).map_err(|e| ErroNuvem::Local(e.to_string()))?;
        }
        std::fs::write(destino, dados).map_err(|e| ErroNuvem::Local(e.to_string()))?;
        *self.baixados.lock().unwrap() += 1;
        Ok(())
    }

    fn apagar(&self, caminho: &str) -> Result<(), ErroNuvem> {
        self.operar()?;
        self.arquivos.lock().unwrap().remove(caminho);
        Ok(())
    }
}
