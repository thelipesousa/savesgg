//! Estado do app compartilhado entre os comandos.

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crate::auto::{AutoStatusView, DestLocks};

use savesgg_core::config::Config;
use savesgg_core::manifest::Manifest;
use savesgg_core::paths::PathContext;
use savesgg_core::scan::{DetectedGame, ScanResult};
use savesgg_core::stores::InstalledGame;

/// Resultado da última varredura.
pub struct ScanData {
    pub result: ScanResult,
    pub ctx: PathContext,
}

impl ScanData {
    pub fn find(&self, id: &str) -> Option<&DetectedGame> {
        self.result
            .games
            .iter()
            .chain(&self.result.over_limit)
            .find(|g| g.id == id)
    }
}

pub struct AppState {
    pub config_path: PathBuf,
    pub config: Mutex<Config>,
    pub manifest: Mutex<Option<Arc<Manifest>>>,
    pub manifest_cache: PathBuf,
    pub manifest_bundled: Vec<PathBuf>,
    pub last_scan: Mutex<Option<Arc<ScanData>>>,
    /// Muda a cada varredura (o motor recria os vigias).
    pub scan_gen: AtomicU64,
    pub locks: DestLocks,
    pub auto_status: Mutex<AutoStatusView>,
    /// `%LOCALAPPDATA%\SavesGG\pendentes`
    pub pending_base: PathBuf,
    /// Pendrives/HDs plugados agora (itens do menu da bandeja).
    pub plugged: Mutex<Vec<savesgg_core::destinations::Destination>>,
    pub drive_watcher: Mutex<crate::drives::DriveWatcher>,
    /// Jogos sem cópia fora do PC (cor da bandeja); atualizado a cada backup.
    pub at_risk_ids: Mutex<std::collections::BTreeSet<String>>,
}

/// Traz a configuração e a fila do 0.1.0 (SaveGuard) para as pastas do SavesGG.
fn migrar_do_nome_antigo(config_path: &std::path::Path) {
    use savesgg_core::legado::{PASTA_DO_APP_ANTIGA, migrar_configuracao, migrar_fila};
    if let Some(nova) = config_path.parent()
        && let Some(raiz) = nova.parent()
    {
        migrar_configuracao(&raiz.join(PASTA_DO_APP_ANTIGA), nova);
    }
    if let Some(local) = dirs::data_local_dir() {
        migrar_fila(&local.join(PASTA_DO_APP_ANTIGA), &local.join("SavesGG"));
    }
}

impl AppState {
    pub fn new(bundled: Vec<PathBuf>) -> Self {
        let config_path = Config::default_path();
        migrar_do_nome_antigo(&config_path);
        let config = Config::load_or_default(&config_path);
        let manifest_cache = config_path.with_file_name("manifest.yaml");
        AppState {
            config_path,
            config: Mutex::new(config),
            manifest: Mutex::new(None),
            manifest_cache,
            manifest_bundled: bundled,
            last_scan: Mutex::new(None),
            scan_gen: AtomicU64::new(0),
            locks: DestLocks::default(),
            auto_status: Mutex::new(AutoStatusView::default()),
            pending_base: dirs::data_local_dir()
                .unwrap_or_default()
                .join("SavesGG")
                .join("pendentes"),
            plugged: Mutex::new(Vec::new()),
            drive_watcher: Mutex::new(crate::drives::DriveWatcher::default()),
            at_risk_ids: Mutex::new(std::collections::BTreeSet::new()),
        }
    }

    pub fn config(&self) -> Config {
        self.config.lock().expect("config").clone()
    }

    /// Altera e grava a configuração.
    pub fn update_config(&self, f: impl FnOnce(&mut Config)) -> Result<(), String> {
        let mut c = self.config.lock().expect("config");
        f(&mut c);
        c.save(&self.config_path).map_err(|e| e.to_string())
    }
}

/// Contexto de caminhos do usuário atual, com o ID da conta Steam (se houver).
pub fn current_ctx(installed: &[InstalledGame]) -> PathContext {
    let mut ctx = PathContext::from_system();
    ctx.store_user_id = installed.iter().find_map(|g| g.store_user_id.clone());
    ctx
}
