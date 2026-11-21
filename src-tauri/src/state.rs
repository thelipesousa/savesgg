//! Estado do app compartilhado entre os comandos.

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crate::auto::{AutoStatusView, DestLocks};

use saveguard_core::config::Config;
use saveguard_core::manifest::Manifest;
use saveguard_core::paths::PathContext;
use saveguard_core::scan::{DetectedGame, ScanResult};
use saveguard_core::stores::InstalledGame;

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
    /// `%LOCALAPPDATA%\SaveGuard\pendentes`
    pub pending_base: PathBuf,
    /// Pendrives/HDs plugados agora (itens do menu da bandeja).
    pub plugged: Mutex<Vec<saveguard_core::destinations::Destination>>,
    pub drive_watcher: Mutex<crate::drives::DriveWatcher>,
    /// Jogos sem cópia fora do PC (cor da bandeja); atualizado a cada backup.
    pub at_risk_ids: Mutex<std::collections::BTreeSet<String>>,
    /// Lista da última varredura (o Início mostra sem precisar escanear de novo).
    pub last_views: Mutex<Option<Vec<crate::views::GameView>>>,
}

impl AppState {
    pub fn new(bundled: Vec<PathBuf>) -> Self {
        let config_path = Config::default_path();
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
                .join("SaveGuard")
                .join("pendentes"),
            plugged: Mutex::new(Vec::new()),
            drive_watcher: Mutex::new(crate::drives::DriveWatcher::default()),
            at_risk_ids: Mutex::new(std::collections::BTreeSet::new()),
            last_views: Mutex::new(None),
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
