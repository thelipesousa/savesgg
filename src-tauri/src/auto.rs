//! Motor do backup automático: vigia processos e pastas, decide quando copiar e
//! manda para os destinos automáticos (ou para a fila de pendentes).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use saveguard_core::CoreError;
use saveguard_core::backup::{BackupOutcome, backup_game, cleanup_temp};
use saveguard_core::config::Config;
use saveguard_core::destinations::{Destination, DestinationKind, auto_targets, list_destinations};
use saveguard_core::pending::{flush_pending, pending_count, pending_root};
use saveguard_core::scan::Source;
use saveguard_core::watch::{
    ProcInfo, ProcessTracker, Scheduler, WatchEvent, games_for_path, watch_roots,
};

#[cfg(test)]
use saveguard_core::paths::PathContext;

use crate::state::{AppState, ScanData};

/// Uma trava por destino: nunca dois backups gravando no mesmo destino ao mesmo tempo.
#[derive(Default)]
pub struct DestLocks(Mutex<HashMap<String, Arc<Mutex<()>>>>);

impl DestLocks {
    pub fn lock(&self, id: &str) -> Arc<Mutex<()>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(id.to_string())
            .or_default()
            .clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoOutcome {
    Copiado {
        game_id: String,
        destination_id: String,
    },
    SemMudanca {
        game_id: String,
        destination_id: String,
    },
    Pendente {
        game_id: String,
        destination_id: String,
    },
    Erro {
        game_id: String,
        message: String,
        /// Vale tentar de novo daqui a pouco (arquivo em uso, save sumindo/mudando).
        retry: bool,
    },
}

/// Estado mostrado na interface e na bandeja.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AutoStatusView {
    pub paused: bool,
    pub running: Vec<String>,
    pub waiting: usize,
    pub pending: usize,
    pub last_backup: Option<String>,
    pub at_risk: usize,
    pub copying: bool,
    pub last_error: Option<String>,
}

/// Faz o backup de um jogo numa pasta de pendentes do destino que está fora.
fn to_pending(
    g: &saveguard_core::scan::DetectedGame,
    scan: &ScanData,
    cfg: &Config,
    destination_id: &str,
    pending_base: &Path,
    locks: &DestLocks,
    now: DateTime<Utc>,
) -> AutoOutcome {
    let pr = pending_root(pending_base, destination_id);
    let lock = locks.lock(&format!("pendente:{destination_id}"));
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    let r = std::fs::create_dir_all(&pr)
        .map_err(CoreError::from)
        .and_then(|_| backup_game(g, &scan.ctx, &pr, cfg.keep_for(&g.id), now));
    match r {
        Ok(_) => AutoOutcome::Pendente {
            game_id: g.id.clone(),
            destination_id: destination_id.to_string(),
        },
        Err(e) => AutoOutcome::Erro {
            game_id: g.id.clone(),
            message: e.to_string(),
            retry: false,
        },
    }
}

/// Executa os backups automáticos vencidos.
pub fn execute_due(
    ids: &[String],
    scan: &ScanData,
    cfg: &Config,
    dests: &[Destination],
    pending_base: &Path,
    locks: &DestLocks,
    now: DateTime<Utc>,
) -> Vec<AutoOutcome> {
    let targets = auto_targets(cfg, dests);
    let mut out = Vec::new();
    for id in ids {
        let Some(g) = scan.find(id) else { continue };
        let confirmed = cfg.confirmed_games.contains(&g.id);
        let grande = scan.result.over_limit.iter().any(|o| o.id == g.id);
        if cfg.ignored_games.contains(&g.id)
            || ((g.source == Source::Heuristica || grande) && !confirmed)
        {
            continue;
        }
        // Saves criados ou apagados desde a varredura (jogos que rodam autosaves).
        let fresh = saveguard_core::scan::refresh_files(g);
        let g = &fresh;
        if g.files.is_empty() {
            continue;
        }
        for d in &targets.available {
            let lock = locks.lock(&d.id);
            let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
            cleanup_temp(&d.root);
            if matches!(d.kind, DestinationKind::PastaSincronizada(_)) {
                let _ = std::fs::create_dir_all(&d.root);
            }
            let res = backup_game(g, &scan.ctx, &d.root, cfg.keep_for(&g.id), now);
            drop(_g);
            out.push(match res {
                Ok(BackupOutcome::Criado(_)) => AutoOutcome::Copiado {
                    game_id: g.id.clone(),
                    destination_id: d.id.clone(),
                },
                Ok(BackupOutcome::SemMudanca) => AutoOutcome::SemMudanca {
                    game_id: g.id.clone(),
                    destination_id: d.id.clone(),
                },
                // Destino sumiu no meio (pendrive arrancado): vai para a fila.
                Err(CoreError::DestinoIndisponivel(_)) => {
                    to_pending(g, scan, cfg, &d.id, pending_base, locks, now)
                }
                Err(CoreError::Io(_)) if !d.root.is_dir() => {
                    to_pending(g, scan, cfg, &d.id, pending_base, locks, now)
                }
                // O problema é na origem (save em uso, sumiu, mudando): tenta de novo.
                Err(
                    e @ (CoreError::Io(_)
                    | CoreError::ArquivoTravado(_)
                    | CoreError::ArquivoMudou(_)),
                ) => AutoOutcome::Erro {
                    game_id: g.id.clone(),
                    message: e.to_string(),
                    retry: true,
                },
                Err(e) => AutoOutcome::Erro {
                    game_id: g.id.clone(),
                    message: e.to_string(),
                    retry: false,
                },
            });
        }
        for uid in &targets.unavailable {
            out.push(to_pending(g, scan, cfg, uid, pending_base, locks, now));
        }
    }
    out
}

/// Envia os pendentes dos destinos automáticos que estão disponíveis. Devolve quantas versões.
pub fn flush_all(
    cfg: &Config,
    dests: &[Destination],
    pending_base: &Path,
    locks: &DestLocks,
) -> usize {
    let mut sent = 0;
    for d in auto_targets(cfg, dests).available {
        let pr = pending_root(pending_base, &d.id);
        if pending_count(&pr) == 0 {
            continue;
        }
        // Ordem fixa das travas (destino, depois a fila) para não haver impasse.
        let lock = locks.lock(&d.id);
        let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
        let plock = locks.lock(&format!("pendente:{}", d.id));
        let _p = plock.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(d.kind, DestinationKind::PastaSincronizada(_)) {
            let _ = std::fs::create_dir_all(&d.root);
        }
        if let Ok(n) = flush_pending(&pr, &d.root) {
            sent += n;
        }
    }
    sent
}

/// Refazer a lista de destinos (registro, unidades, IOCTL USB) só quando um disco
/// entrou/saiu, a configuração mudou ou a cada 60 s — não a cada volta de 5 s.
pub fn needs_dest_refresh(
    prev_mask: Option<u32>,
    mask: u32,
    last_refresh: u64,
    now: u64,
    cfg_changed: bool,
) -> bool {
    prev_mask != Some(mask) || cfg_changed || now >= last_refresh + 60
}

/// Jogo copiado para um destino fora do PC deixa de estar em risco.
pub fn clear_protected(
    at_risk: &mut std::collections::BTreeSet<String>,
    out: &[AutoOutcome],
    dests: &[Destination],
) {
    for o in out {
        if let AutoOutcome::Copiado {
            game_id,
            destination_id,
        }
        | AutoOutcome::SemMudanca {
            game_id,
            destination_id,
        } = o
            && dests.iter().any(|d| {
                &d.id == destination_id && saveguard_core::destinations::counts_as_off_pc(d)
            })
        {
            at_risk.remove(game_id);
        }
    }
}

/// Total de versões esperando na fila (todos os destinos automáticos).
fn total_pending(cfg: &Config, pending_base: &Path) -> usize {
    cfg.auto_destinations
        .iter()
        .map(|id| pending_count(&pending_root(pending_base, id)))
        .sum()
}

/// Vigia as pastas; devolve as que falharam (sumiram, sem permissão) sem parar.
pub fn try_watch(w: &mut impl Watcher, roots: &[PathBuf]) -> Vec<(PathBuf, String)> {
    roots
        .iter()
        .filter_map(|r| {
            w.watch(r, RecursiveMode::Recursive)
                .err()
                .map(|e| (r.clone(), e.to_string()))
        })
        .collect()
}

#[cfg(windows)]
fn background_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
    };
    // SAFETY: só altera a prioridade da própria thread.
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}

#[cfg(not(windows))]
fn background_priority() {}

fn processes(sys: &mut sysinfo::System) -> Vec<ProcInfo> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.processes()
        .iter()
        .filter_map(|(pid, p)| {
            p.exe().map(|e| ProcInfo {
                pid: pid.as_u32(),
                exe: e.to_path_buf(),
            })
        })
        .collect()
}

/// Atualiza o estado compartilhado e avisa a interface (e a bandeja) se mudou.
pub fn publish(app: &AppHandle, f: impl FnOnce(&mut AutoStatusView)) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut s = state.auto_status.lock().unwrap_or_else(|e| e.into_inner());
        let before = s.clone();
        f(&mut s);
        if *s == before {
            return;
        }
        s.clone()
    };
    let _ = app.emit("status-automatico", &snapshot);
    crate::on_status_changed(app, &snapshot);
}

/// "Fazer backup agora" da bandeja: todos os jogos para os destinos automáticos.
pub fn backup_all_now(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(scan) = state
        .last_scan
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    else {
        crate::notify_user(
            app,
            "SaveGuard",
            "Ainda escaneando o PC; tente de novo em instantes.",
        );
        return;
    };
    let cfg = state.config();
    if cfg.auto_destinations.is_empty() {
        crate::notify_user(
            app,
            "SaveGuard",
            "Escolha os destinos do backup automático em Configurações.",
        );
        return;
    }
    publish(app, |s| s.copying = true);
    let ids = crate::commands::all_backup_ids(&scan, &cfg);
    let dests = list_destinations(&cfg);
    let out = execute_due(
        &ids,
        &scan,
        &cfg,
        &dests,
        &state.pending_base,
        &state.locks,
        Utc::now(),
    );
    let erros = out
        .iter()
        .filter(|o| matches!(o, AutoOutcome::Erro { .. }))
        .count();
    if erros > 0 {
        crate::notify_user(
            app,
            "Backup não concluído",
            &format!("{erros} jogo(s) não puderam ser copiados agora."),
        );
    }
    publish(app, |s| {
        s.copying = false;
        s.last_backup = Some(Utc::now().to_rfc3339());
    });
}

/// Inicia o motor numa thread própria, em prioridade baixa.
pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("saveguard-automatico".into())
        .spawn(move || {
            background_priority();
            let state = app.state::<AppState>();
            match crate::commands::run_scan_blocking(&state) {
                Ok(views) => {
                    let _ = app.emit("varredura-concluida", &views);
                }
                Err(e) => publish(&app, |s| s.last_error = Some(e)),
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(move |r| {
                let _ = tx.send(r);
            })
            .ok();
            // Sem acesso aos processos, o vigia de pastas segue cobrindo os jogos.
            let mut sys = sysinfo::System::new().ok();
            let mut scheduler = Scheduler::new();
            let mut tracker = ProcessTracker::new(&[]);
            let mut watched: Vec<PathBuf> = Vec::new();
            let mut seen_gen = u64::MAX;
            let start = Instant::now();
            let mut last_flush = 0u64;
            // Lista de destinos em cache: refeita só quando precisa (ver needs_dest_refresh).
            let mut dests: Vec<Destination> = Vec::new();
            let mut last_mask: Option<u32> = None;
            let mut last_dest_refresh = 0u64;
            let mut last_dest_cfg: Option<(Vec<String>, Vec<PathBuf>)> = None;
            loop {
                let now_s = start.elapsed().as_secs();
                let scan = state.last_scan.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let gen_now = state.scan_gen.load(Ordering::Relaxed);
                if let Some(scan) = &scan
                    && gen_now != seen_gen
                {
                    seen_gen = gen_now;
                    let games: Vec<_> =
                        scan.result.games.iter().chain(&scan.result.over_limit).cloned().collect();
                    // Lembra quem estava aberto: se fechou durante a varredura, ainda conta.
                    tracker = ProcessTracker::with_running(&games, tracker.running());
                    if let Some(w) = watcher.as_mut() {
                        for r in &watched {
                            let _ = w.unwatch(r);
                        }
                        watched = watch_roots(&games);
                        for (p, e) in try_watch(w, &watched) {
                            eprintln!("vigia: não deu para vigiar {}: {e}", p.display());
                        }
                    }
                }
                let cfg = state.config();
                let mask = saveguard_core::destinations::drive_mask();
                let dest_cfg = (cfg.auto_destinations.clone(), cfg.custom_folders.clone());
                let cfg_changed = last_dest_cfg.as_ref() != Some(&dest_cfg);
                if needs_dest_refresh(last_mask, mask, last_dest_refresh, now_s, cfg_changed) {
                    dests = list_destinations(&cfg);
                    last_mask = Some(mask);
                    last_dest_refresh = now_s;
                    last_dest_cfg = Some(dest_cfg);
                    // Discos externos: avisa quando um entra ou sai.
                    crate::drives::poll(&app, &dests);
                }
                let procs = sys.as_mut().map(processes).unwrap_or_default();
                for ev in tracker.update(&procs) {
                    scheduler.on_event(ev, now_s);
                }
                if let Some(scan) = &scan {
                    let games: Vec<_> =
                        scan.result.games.iter().chain(&scan.result.over_limit).cloned().collect();
                    for ev in rx.try_iter().flatten() {
                        if ev.kind.is_access() {
                            continue;
                        }
                        for p in &ev.paths {
                            for id in games_for_path(p, &games) {
                                scheduler.on_event(WatchEvent::PastaMudou(id), now_s);
                            }
                        }
                    }
                }
                if !cfg.auto_paused
                    && let Some(scan) = &scan
                {
                    let due = scheduler.due(now_s);
                    if !due.is_empty() {
                        publish(&app, |s| s.copying = true);
                        let out = execute_due(&due, scan, &cfg, &dests, &state.pending_base, &state.locks, Utc::now());
                        let copied = out.iter().any(|o| matches!(o, AutoOutcome::Copiado { .. }));
                        let mut err = None;
                        for o in &out {
                            match o {
                                AutoOutcome::Copiado { game_id, .. } | AutoOutcome::SemMudanca { game_id, .. } => {
                                    scheduler.succeeded(game_id);
                                }
                                AutoOutcome::Erro { game_id, message, retry } => {
                                    // Arquivo em uso logo depois de fechar: tenta de novo em
                                    // 30 s, 2 min e 10 min; só avisa quando desistir.
                                    if !(*retry && scheduler.retry(game_id, now_s)) {
                                        let nome = scan.find(game_id).map_or(game_id.as_str(), |g| g.name.as_str());
                                        err = Some(format!("{nome}: {message}"));
                                    }
                                }
                                AutoOutcome::Pendente { .. } => {}
                            }
                        }
                        if let Some(e) = &err {
                            crate::notify_user(&app, "Backup automático não concluído", e);
                        }
                        let at_risk = {
                            let mut ids = state.at_risk_ids.lock().unwrap_or_else(|e| e.into_inner());
                            clear_protected(&mut ids, &out, &dests);
                            ids.len()
                        };
                        publish(&app, |s| {
                            s.copying = false;
                            s.at_risk = at_risk;
                            if copied {
                                s.last_backup = Some(Utc::now().to_rfc3339());
                            }
                            if err.is_some() {
                                s.last_error = err;
                            }
                        });
                    }
                    if now_s >= last_flush + 60 {
                        last_flush = now_s;
                        let n = flush_all(&cfg, &dests, &state.pending_base, &state.locks);
                        if n > 0 {
                            crate::notify_user(&app, "Backups enviados", &format!("{n} backup(s) que estavam esperando foram copiados para o destino."));
                        }
                    }
                }
                let running: Vec<String> = scheduler
                    .running()
                    .into_iter()
                    .map(|id| scan.as_ref().and_then(|s| s.find(&id)).map_or(id, |g| g.name.clone()))
                    .collect();
                let pending = total_pending(&cfg, &state.pending_base);
                publish(&app, |s| {
                    s.paused = cfg.auto_paused;
                    s.running = running;
                    s.waiting = scheduler.pending_count();
                    s.pending = pending;
                });
                std::thread::sleep(Duration::from_secs(5));
            }
        })
        .expect("não foi possível iniciar o motor automático");
}

#[cfg(test)]
mod tests {
    use super::*;
    use saveguard_core::destinations::build_destinations;
    use saveguard_core::manifest::{CloudFlags, Store};
    use saveguard_core::pending::pending_count;
    use saveguard_core::scan::{DetectedGame, ScanResult, Source};
    use std::fs;
    use std::path::{Path, PathBuf};

    struct Fx {
        t: tempfile::TempDir,
        scan: ScanData,
    }

    fn fx() -> Fx {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("Users/Ana");
        let ctx = PathContext {
            app_data: home.join("AppData/Roaming"),
            local_app_data: home.join("AppData/Local"),
            local_app_data_low: home.join("AppData/LocalLow"),
            documents: home.join("Documents"),
            public: t.path().join("Users/Public"),
            program_data: t.path().join("ProgramData"),
            os_user_name: "Ana".into(),
            store_user_id: None,
            home,
        };
        let save = ctx.app_data.join("Jogo");
        fs::create_dir_all(&save).unwrap();
        fs::write(save.join("a.sav"), b"slot").unwrap();
        let game = DetectedGame {
            id: "jogo".into(),
            name: "Jogo".into(),
            store: Store::SemLoja,
            save_roots: vec![save.clone()],
            files: vec![save.join("a.sav")],
            total_bytes: 4,
            cloud: CloudFlags::default(),
            source: Source::Manifesto,
            exe_hint: None,
            base: None,
            root: None,
            store_game_id: None,
            patterns: vec![save.to_string_lossy().replace('\\', "/")],
            steam_app_id: None,
        };
        let scan = ScanData {
            result: ScanResult {
                games: vec![game],
                over_limit: vec![],
            },
            ctx,
        };
        Fx { t, scan }
    }

    fn cfg_with(ids: Vec<String>) -> Config {
        Config {
            auto_destinations: ids,
            ..Default::default()
        }
    }

    fn pasta(p: &Path) -> Vec<saveguard_core::destinations::Destination> {
        build_destinations(&[], &[], &[p.to_path_buf()])
    }

    #[test]
    fn execute_due_copia_para_destinos_automaticos() {
        let f = fx();
        let dest = f.t.path().join("Destino");
        fs::create_dir_all(&dest).unwrap();
        let dests = pasta(&dest);
        let cfg = cfg_with(vec![dests[0].id.clone()]);
        let out = execute_due(
            &["jogo".into()],
            &f.scan,
            &cfg,
            &dests,
            &f.t.path().join("pend"),
            &DestLocks::default(),
            Utc::now(),
        );
        assert!(matches!(&out[..], [AutoOutcome::Copiado { .. }]), "{out:?}");
        assert_eq!(
            saveguard_core::backup::LibraryIndex::load(&dest)
                .unwrap()
                .games["jogo"]
                .len(),
            1
        );
    }

    #[test]
    fn destino_some_vira_pendente() {
        let f = fx();
        let dest = f.t.path().join("Pendrive");
        let dests = pasta(&dest); // a pasta não existe: indisponível
        let cfg = cfg_with(vec![dests[0].id.clone()]);
        let base = f.t.path().join("pend");
        let locks = DestLocks::default();
        let out = execute_due(
            &["jogo".into()],
            &f.scan,
            &cfg,
            &dests,
            &base,
            &locks,
            Utc::now(),
        );
        assert!(
            matches!(&out[..], [AutoOutcome::Pendente { .. }]),
            "{out:?}"
        );
        let pr = saveguard_core::pending::pending_root(&base, &dests[0].id);
        assert_eq!(pending_count(&pr), 1);
        // o destino volta
        fs::create_dir_all(&dest).unwrap();
        let dests = pasta(&dest);
        assert_eq!(flush_all(&cfg, &dests, &base, &locks), 1);
        assert_eq!(pending_count(&pr), 0);
    }

    #[test]
    fn automatico_pega_save_novo_e_ignora_apagado() {
        let f = fx();
        let save_dir = f.scan.result.games[0].save_roots[0].clone();
        fs::remove_file(save_dir.join("a.sav")).unwrap();
        fs::write(save_dir.join("b.sav"), b"novo").unwrap();
        let dest = f.t.path().join("Destino");
        fs::create_dir_all(&dest).unwrap();
        let dests = pasta(&dest);
        let cfg = cfg_with(vec![dests[0].id.clone()]);
        let out = execute_due(
            &["jogo".into()],
            &f.scan,
            &cfg,
            &dests,
            &f.t.path().join("p"),
            &DestLocks::default(),
            Utc::now(),
        );
        assert!(matches!(&out[..], [AutoOutcome::Copiado { .. }]), "{out:?}");
        let idx = saveguard_core::backup::LibraryIndex::load(&dest).unwrap();
        let stem = &idx.games["jogo"][0].stem;
        let meta = saveguard_core::backup::load_meta(&dest, "jogo", stem).unwrap();
        assert!(
            meta.files.iter().any(|e| e.template.ends_with("b.sav")),
            "{:?}",
            meta.files
        );
        assert!(!meta.files.iter().any(|e| e.template.ends_with("a.sav")));
    }

    #[test]
    fn erro_na_origem_nao_vira_pendente() {
        let mut f = fx();
        f.scan.result.games[0].patterns.clear(); // sem padrões: usa a lista antiga
        fs::remove_file(&f.scan.result.games[0].files[0]).unwrap();
        let dest = f.t.path().join("Destino");
        fs::create_dir_all(&dest).unwrap();
        let dests = pasta(&dest);
        let cfg = cfg_with(vec![dests[0].id.clone()]);
        let base = f.t.path().join("p");
        let out = execute_due(
            &["jogo".into()],
            &f.scan,
            &cfg,
            &dests,
            &base,
            &DestLocks::default(),
            Utc::now(),
        );
        assert!(
            matches!(&out[..], [AutoOutcome::Erro { retry: true, .. }]),
            "{out:?}"
        );
        assert_eq!(
            pending_count(&saveguard_core::pending::pending_root(&base, &dests[0].id)),
            0
        );
    }

    #[test]
    fn destinos_so_sao_refeitos_quando_precisa() {
        assert!(needs_dest_refresh(None, 0b100, 0, 0, false), "primeira vez");
        assert!(!needs_dest_refresh(Some(0b100), 0b100, 10, 15, false));
        assert!(
            needs_dest_refresh(Some(0b100), 0b110, 10, 15, false),
            "disco novo"
        );
        assert!(
            needs_dest_refresh(Some(0b100), 0b100, 10, 70, false),
            "a cada 60 s"
        );
        assert!(
            needs_dest_refresh(Some(0b100), 0b100, 10, 15, true),
            "config mudou"
        );
    }

    #[test]
    fn copia_fora_do_pc_tira_o_jogo_do_risco() {
        let t = tempfile::tempdir().unwrap();
        let mut risco: std::collections::BTreeSet<String> = ["a".into(), "b".into()].into();
        let local = pasta(t.path());
        let pen = build_destinations(
            &[],
            &[saveguard_core::destinations::DriveInfo {
                root: t.path().to_path_buf(),
                label: "PEN".into(),
                serial: 7,
                free_bytes: None,
            }],
            &[],
        );
        let todos: Vec<_> = local.iter().chain(&pen).cloned().collect();
        let out = vec![
            AutoOutcome::Copiado {
                game_id: "a".into(),
                destination_id: local[0].id.clone(),
            },
            AutoOutcome::Copiado {
                game_id: "b".into(),
                destination_id: pen[0].id.clone(),
            },
        ];
        clear_protected(&mut risco, &out, &todos);
        assert_eq!(
            risco,
            ["a".to_string()].into(),
            "pasta local não protege; pendrive sim"
        );
    }

    #[test]
    fn sem_destino_automatico_nao_faz_nada() {
        let f = fx();
        let out = execute_due(
            &["jogo".into()],
            &f.scan,
            &cfg_with(vec![]),
            &[],
            &f.t.path().join("p"),
            &DestLocks::default(),
            Utc::now(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn possivel_save_nao_confirmado_fica_de_fora() {
        let mut f = fx();
        f.scan.result.games[0].source = Source::Heuristica;
        let dest = f.t.path().join("Destino");
        fs::create_dir_all(&dest).unwrap();
        let dests = pasta(&dest);
        let mut cfg = cfg_with(vec![dests[0].id.clone()]);
        let out = execute_due(
            &["jogo".into()],
            &f.scan,
            &cfg,
            &dests,
            &f.t.path().join("p"),
            &DestLocks::default(),
            Utc::now(),
        );
        assert!(out.is_empty(), "{out:?}");
        cfg.confirmed_games.push("jogo".into());
        let out = execute_due(
            &["jogo".into()],
            &f.scan,
            &cfg,
            &dests,
            &f.t.path().join("p"),
            &DestLocks::default(),
            Utc::now(),
        );
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn trava_por_destino_serializa() {
        // Dois jogos com backup ao mesmo tempo no mesmo destino: sem trava, um
        // gravaria o índice por cima do outro e uma versão sumiria da lista.
        let mut f = fx();
        let mut g2 = f.scan.result.games[0].clone();
        g2.id = "jogo2".into();
        let save2 = f.t.path().join("Saves2");
        fs::create_dir_all(&save2).unwrap();
        fs::write(save2.join("b.sav"), b"outro").unwrap();
        g2.save_roots = vec![save2.clone()];
        g2.files = vec![save2.join("b.sav")];
        f.scan.result.games.push(g2);
        let dest = f.t.path().join("Destino");
        fs::create_dir_all(&dest).unwrap();
        let dests = pasta(&dest);
        let cfg = cfg_with(vec![dests[0].id.clone()]);
        let locks = std::sync::Arc::new(DestLocks::default());
        let scan = std::sync::Arc::new(f.scan);
        let base = f.t.path().join("p");
        for rodada in 0..5i64 {
            for g in &scan.result.games {
                fs::write(&g.files[0], format!("rodada {rodada}")).unwrap();
            }
            let hs: Vec<_> = ["jogo", "jogo2"]
                .into_iter()
                .map(|id| {
                    let (locks, scan, cfg, dests, base) = (
                        locks.clone(),
                        scan.clone(),
                        cfg.clone(),
                        dests.clone(),
                        base.clone(),
                    );
                    std::thread::spawn(move || {
                        execute_due(
                            &[id.to_string()],
                            &scan,
                            &cfg,
                            &dests,
                            &base,
                            &locks,
                            Utc::now() + chrono::Duration::seconds(rodada),
                        )
                    })
                })
                .collect();
            for h in hs {
                h.join().unwrap();
            }
        }
        let idx = saveguard_core::backup::LibraryIndex::load(&dest).unwrap();
        let raw: saveguard_core::backup::LibraryIndex = serde_json::from_str(
            &fs::read_to_string(dest.join("SaveGuard/biblioteca.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(idx.games["jogo"].len(), 5);
        assert_eq!(idx.games["jogo2"].len(), 5);
        assert_eq!(
            raw, idx,
            "o arquivo do índice não pode ter perdido entradas"
        );
    }

    #[test]
    fn motor_segue_quando_pasta_some() {
        let t = tempfile::tempdir().unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut w = notify::recommended_watcher(move |r| {
            let _ = tx.send(r);
        })
        .unwrap();
        let roots = vec![t.path().to_path_buf(), PathBuf::from("Z:/nao/existe")];
        let erros = try_watch(&mut w, &roots);
        assert_eq!(erros.len(), 1);
        assert_eq!(erros[0].0, PathBuf::from("Z:/nao/existe"));
    }
}
