//! Contas de nuvem no app: provedor de cada conta, enviador em segundo plano e status.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use savesgg_core::backup::{LibraryIndex, SnapshotRef};
use savesgg_core::config::Config;
use savesgg_core::nuvem::gdrive::GoogleDrive;
use savesgg_core::nuvem::http::HttpUreq;
use savesgg_core::nuvem::sync::{ResumoSync, pendentes, sincronizar_com_trava, ultimas_enviadas};
use savesgg_core::nuvem::{ContaNuvem, ErroNuvem, Provedor, ProvedorNuvem, pasta_preparacao};

use crate::oauth::{self, TokenKeyring};
use crate::state::AppState;

/// Intervalo entre passadas quando há algo esperando envio.
const INTERVALO: Duration = Duration::from_secs(10 * 60);
/// Espera depois de uma falha de rede ou da nuvem: 30 s, 2 min, 10 min.
const ESPERAS: [Duration; 3] = [
    Duration::from_secs(30),
    Duration::from_secs(120),
    Duration::from_secs(600),
];

/// Estado de envio de uma conta, mostrado em Configurações e usado pela bandeja.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StatusNuvem {
    pub destination_id: String,
    pub pendentes: usize,
    pub enviando: bool,
    pub erro: Option<String>,
    pub precisa_entrar: bool,
}

/// Atualiza o status de uma conta com o resultado de uma passada.
pub fn aplicar_resultado(st: &mut StatusNuvem, r: &Result<ResumoSync, ErroNuvem>) {
    st.enviando = false;
    match r {
        Ok(resumo) => {
            st.pendentes = resumo.pendentes;
            st.erro = None;
            st.precisa_entrar = false;
        }
        Err(ErroNuvem::Login) => {
            st.precisa_entrar = true;
            st.erro = Some(ErroNuvem::Login.to_string());
        }
        Err(e) => st.erro = Some(e.to_string()),
    }
}

/// `%LOCALAPPDATA%\SavesGG`.
pub fn base_local() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("SavesGG")
}

pub fn preparacao(conta: &ContaNuvem) -> PathBuf {
    pasta_preparacao(&base_local(), &conta.destino_id())
}

/// Conta dona de um destino `nuvem:*`.
pub fn conta_do_destino(cfg: &Config, destination_id: &str) -> Option<ContaNuvem> {
    cfg.contas_nuvem
        .iter()
        .find(|c| c.destino_id() == destination_id)
        .cloned()
}

/// Provedor de verdade de uma conta (falha se o login não está neste build).
pub fn provedor_de(conta: &ContaNuvem) -> Result<Box<dyn Provedor>, ErroNuvem> {
    let cliente = oauth::cliente(conta.provedor)
        .ok_or_else(|| ErroNuvem::Local("Login na nuvem indisponível nesta versão".into()))?;
    match conta.provedor {
        ProvedorNuvem::Gdrive => Ok(Box::new(GoogleDrive::new(
            HttpUreq::default(),
            TokenKeyring::new(conta.destino_id(), cliente),
            preparacao(conta).join("ids.json"),
        ))),
    }
}

/// Índice só com a versão mais recente já enviada de cada jogo (para a proteção:
/// versão que ainda não subiu não conta como cópia fora do PC).
pub fn indice_enviado(prep: &std::path::Path) -> LibraryIndex {
    let mut idx = LibraryIndex::default();
    for (jogo, quando) in ultimas_enviadas(prep) {
        idx.games.insert(
            jogo,
            vec![SnapshotRef {
                stem: String::new(),
                created: quando,
                pinned: false,
                content_hash: String::new(),
                total_bytes: 0,
                seguranca: false,
            }],
        );
    }
    idx
}

/// Acorda o enviador (depois de um backup, do "Enviar agora" ou ao entrar numa conta).
#[derive(Default)]
pub struct Enviador {
    tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl Enviador {
    pub fn avisar(&self) {
        if let Some(tx) = self.tx.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(());
        }
    }
}

fn publicar(app: &AppHandle, st: StatusNuvem) {
    crate::auto::publish(app, |s| {
        match s
            .nuvem
            .iter_mut()
            .find(|x| x.destination_id == st.destination_id)
        {
            Some(x) => *x = st,
            None => s.nuvem.push(st),
        }
    });
}

/// Inicia o enviador em segundo plano.
pub fn iniciar(app: AppHandle) {
    let (tx, rx) = mpsc::channel::<()>();
    *app.state::<AppState>()
        .enviador
        .tx
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(tx);
    let _ = std::thread::Builder::new()
        .name("savesgg-nuvem".into())
        .spawn(move || {
            let mut tentativas: HashMap<String, (usize, Instant)> = HashMap::new();
            // Primeira passada logo ao abrir.
            let mut acordar = true;
            loop {
                if !acordar {
                    let espera = tentativas
                        .values()
                        .map(|(_, ate)| ate.saturating_duration_since(Instant::now()))
                        .min()
                        .unwrap_or(INTERVALO)
                        .min(INTERVALO);
                    match rx.recv_timeout(espera) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                    while rx.try_recv().is_ok() {}
                }
                acordar = false;
                let state = app.state::<AppState>();
                let cfg = state.config();
                let ativos: Vec<String> = cfg.contas_nuvem.iter().map(|c| c.destino_id()).collect();
                crate::auto::publish(&app, |s| {
                    s.nuvem.retain(|x| ativos.contains(&x.destination_id))
                });
                for conta in &cfg.contas_nuvem {
                    let id = conta.destino_id();
                    let prep = preparacao(conta);
                    let n = pendentes(&prep);
                    let mut st = state
                        .auto_status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .nuvem
                        .iter()
                        .find(|x| x.destination_id == id)
                        .cloned()
                        .unwrap_or(StatusNuvem {
                            destination_id: id.clone(),
                            ..Default::default()
                        });
                    st.pendentes = n;
                    if n == 0 && st.erro.is_none() {
                        publicar(&app, st);
                        continue;
                    }
                    st.enviando = true;
                    publicar(&app, st.clone());
                    let lock = state.locks.lock(&id);
                    // A trava do destino só vale nos passos locais: o envio roda sem
                    // ela, para o backup local não ficar esperando a internet.
                    let r = provedor_de(conta)
                        .and_then(|p| sincronizar_com_trava(&prep, p.as_ref(), &lock));
                    match &r {
                        Ok(resumo) => {
                            log::info!(
                                "nuvem: {} enviados, {} apagados, {} pendentes",
                                resumo.enviados,
                                resumo.apagados,
                                resumo.pendentes
                            );
                            tentativas.remove(&id);
                        }
                        Err(ErroNuvem::Login) => {
                            log::warn!("nuvem: a conta precisa entrar de novo");
                            tentativas.remove(&id);
                        }
                        Err(e) => {
                            log::warn!("nuvem: falha no envio: {e}");
                            let t = tentativas.get(&id).map_or(0, |(t, _)| *t);
                            let espera = ESPERAS[t.min(ESPERAS.len() - 1)];
                            tentativas.insert(id.clone(), (t + 1, Instant::now() + espera));
                        }
                    }
                    aplicar_resultado(&mut st, &r);
                    st.pendentes = pendentes(&prep);
                    publicar(&app, st);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_nuvem_marca_precisa_entrar() {
        let mut st = StatusNuvem {
            destination_id: "nuvem:gdrive:abc".into(),
            pendentes: 3,
            enviando: true,
            ..Default::default()
        };
        aplicar_resultado(&mut st, &Err(ErroNuvem::Login));
        assert!(st.precisa_entrar);
        assert!(!st.enviando);
        assert_eq!(st.erro.as_deref(), Some("entre de novo na conta"));
        aplicar_resultado(
            &mut st,
            &Ok(ResumoSync {
                enviados: 3,
                apagados: 0,
                pendentes: 0,
            }),
        );
        assert!(!st.precisa_entrar);
        assert_eq!(st.erro, None);
        assert_eq!(st.pendentes, 0);
    }

    #[test]
    fn indice_enviado_so_conta_versoes_na_nuvem() {
        use savesgg_core::protection::BackupHistory;
        let t = tempfile::tempdir().unwrap();
        let prep = t.path().join("prep");
        let jogo = prep.join("SavesGG/hades");
        std::fs::create_dir_all(&jogo).unwrap();
        let stem = "2026-11-20_21-40-05";
        let meta = serde_json::json!({
            "game_id": "hades", "game_name": "Hades", "created": "2026-11-20T21:40:05Z",
            "source_pc": "PC", "files": [], "total_bytes": 0, "content_hash": "h",
            "pinned": false, "store": "sem_loja", "store_user_id": null
        });
        std::fs::write(jogo.join(format!("{stem}.json")), meta.to_string()).unwrap();
        assert_eq!(indice_enviado(&prep).last_backup("hades"), None);
        let estado = serde_json::json!({ "arquivos": {
            format!("SavesGG/hades/{stem}.zip"): { "blake3": "a", "tamanho": 1 },
            format!("SavesGG/hades/{stem}.json"): { "blake3": "b", "tamanho": 1 }
        }});
        std::fs::write(prep.join("estado.json"), estado.to_string()).unwrap();
        assert!(indice_enviado(&prep).last_backup("hades").is_some());
    }
}
