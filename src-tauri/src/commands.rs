//! Comandos chamados pela interface. Finos: a lógica fica em `saveguard-core`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

use saveguard_core::backup::{BackupOutcome, LibraryIndex, backup_game, load_meta};
use saveguard_core::destinations::{
    Destination, DestinationKind, counts_as_off_pc, list_destinations,
};
use saveguard_core::protection::{BackupHistory, assess, sort_for_display};
use saveguard_core::restore::{
    RestorePlan, RestoreReport, apply_restore as core_apply, plan_restore as core_plan,
    resolve_overrides, vars_for,
};
use saveguard_core::scan::{ScanOptions, find_emulator_roots, scan as core_scan};
use saveguard_core::stores::detect_all;
use saveguard_core::sync_folders::detect_sync_roots;

use crate::manifest_source;
use crate::state::{AppState, ScanData, current_ctx};
use crate::views::{GameView, game_view};

type Res<T> = Result<T, String>;

/// Junta o histórico de todos os destinos disponíveis.
struct MultiHistory(Vec<LibraryIndex>);

impl BackupHistory for MultiHistory {
    fn last_backup(&self, game_id: &str) -> Option<DateTime<Utc>> {
        self.0.iter().filter_map(|i| i.last_backup(game_id)).max()
    }
}

fn history(dests: &[Destination]) -> MultiHistory {
    MultiHistory(
        dests
            .iter()
            .filter(|d| d.available && d.has_backup && counts_as_off_pc(d))
            .filter_map(|d| LibraryIndex::load(&d.root).ok())
            .collect(),
    )
}

fn drive_roots() -> Vec<PathBuf> {
    (b'C'..=b'Z')
        .map(|l| PathBuf::from(format!("{}:\\", l as char)))
        .filter(|p| p.is_dir())
        .collect()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "erro interno".to_string())?
}

/// Varre o PC (bloqueante): carrega o manifesto se preciso, guarda o resultado no
/// estado e devolve os jogos já ordenados por prioridade. Usada pelo comando e pelo motor.
pub fn run_scan_blocking(state: &AppState) -> Res<Vec<GameView>> {
    let cached = state
        .manifest
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let manifest = match cached {
        Some(m) => m,
        None => {
            let m = Arc::new(manifest_source::load(
                &state.manifest_cache,
                &state.manifest_bundled,
            )?);
            *state.manifest.lock().unwrap_or_else(|e| e.into_inner()) = Some(m.clone());
            m
        }
    };
    let cfg = state.config();
    let installed = detect_all();
    let ctx = current_ctx(&installed);
    let opts = ScanOptions {
        extra_roots: find_emulator_roots(&drive_roots()),
        manual: cfg.manual_games.clone(),
    };
    let result = core_scan(&ctx, &manifest, &installed, &opts);
    let dests = list_destinations(&cfg);
    let hist = history(&dests);
    let sync = detect_sync_roots();
    let now = Utc::now();
    let visible = |g: &&saveguard_core::scan::DetectedGame| !cfg.ignored_games.contains(&g.id);
    let mut pairs: Vec<_> = result
        .games
        .iter()
        .filter(visible)
        .map(|g| (g.clone(), assess(g, &sync, &hist, now)))
        .collect();
    sort_for_display(&mut pairs);
    let mut views: Vec<GameView> = pairs
        .iter()
        .map(|(g, a)| {
            game_view(
                g,
                a,
                &ctx,
                hist.last_backup(&g.id),
                false,
                cfg.confirmed_games.contains(&g.id),
            )
        })
        .collect();
    for g in result.over_limit.iter().filter(visible) {
        let a = assess(g, &sync, &hist, now);
        views.push(game_view(
            g,
            &a,
            &ctx,
            hist.last_backup(&g.id),
            true,
            cfg.confirmed_games.contains(&g.id),
        ));
    }
    let at_risk = views
        .iter()
        .filter(|v| !v.grande && v.priority == saveguard_core::protection::Priority::Alta)
        .count();
    *state.last_scan.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(Arc::new(ScanData { result, ctx }));
    state
        .scan_gen
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .auto_status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .at_risk = at_risk;
    Ok(views)
}

/// Varre o PC e devolve os jogos já ordenados por prioridade.
#[tauri::command]
pub async fn scan(app: tauri::AppHandle) -> Res<Vec<GameView>> {
    blocking(move || run_scan_blocking(&app.state::<AppState>())).await
}

#[tauri::command]
pub async fn list_destinations_cmd(state: State<'_, AppState>) -> Res<Vec<Destination>> {
    let cfg = state.config();
    blocking(move || Ok(list_destinations(&cfg))).await
}

/// Abre o seletor de pasta e adiciona a pasta como destino.
#[tauri::command]
pub async fn add_custom_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Res<Option<Destination>> {
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|_| "pasta inválida".to_string())?;
    state.update_config(|c| {
        if !c.custom_folders.contains(&path) {
            c.custom_folders.push(path.clone());
        }
    })?;
    let cfg = state.config();
    Ok(list_destinations(&cfg).into_iter().find(|d| d.root == path))
}

/// Seletor de pasta genérico (restauração de itens não resolvidos).
#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle) -> Res<Option<String>> {
    Ok(app
        .dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[derive(Debug, Serialize)]
pub struct BackupResultView {
    pub game_id: String,
    pub destination_id: String,
    /// "criado" | "sem_mudanca" | "erro"
    pub status: &'static str,
    pub message: Option<String>,
}

fn find_destinations(cfg: &saveguard_core::config::Config, ids: &[String]) -> Vec<Destination> {
    list_destinations(cfg)
        .into_iter()
        .filter(|d| ids.contains(&d.id))
        .collect()
}

#[tauri::command]
pub async fn backup_now(
    state: State<'_, AppState>,
    game_ids: Vec<String>,
    destination_ids: Vec<String>,
) -> Res<Vec<BackupResultView>> {
    let data = state
        .last_scan
        .lock()
        .expect("scan")
        .clone()
        .ok_or("faça uma varredura primeiro")?;
    let cfg = state.config();
    blocking(move || {
        let dests = find_destinations(&cfg, &destination_ids);
        let mut out = Vec::new();
        for d in &dests {
            if matches!(d.kind, DestinationKind::PastaSincronizada(_))
                && d.available
                && let Err(e) = std::fs::create_dir_all(&d.root)
            {
                out.push(BackupResultView {
                    game_id: String::new(),
                    destination_id: d.id.clone(),
                    status: "erro",
                    message: Some(e.to_string()),
                });
                continue;
            }
            for id in &game_ids {
                let Some(g) = data.find(id) else { continue };
                let r = backup_game(g, &data.ctx, &d.root, cfg.keep_for(id), Utc::now());
                let (status, message) = match r {
                    Ok(BackupOutcome::Criado(_)) => ("criado", None),
                    Ok(BackupOutcome::SemMudanca) => ("sem_mudanca", None),
                    Err(e) => ("erro", Some(e.to_string())),
                };
                out.push(BackupResultView {
                    game_id: id.clone(),
                    destination_id: d.id.clone(),
                    status,
                    message,
                });
            }
        }
        Ok(out)
    })
    .await
}

#[derive(Debug, Serialize)]
pub struct SnapshotView {
    pub stem: String,
    pub created: String,
    pub pinned: bool,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct GameSnapshotsView {
    pub game_id: String,
    pub game_name: String,
    pub snapshots: Vec<SnapshotView>,
}

fn destination_root(state: &AppState, destination_id: &str) -> Res<PathBuf> {
    let cfg = state.config();
    find_destinations(&cfg, &[destination_id.to_string()])
        .into_iter()
        .find(|d| d.available)
        .map(|d| d.root)
        .ok_or_else(|| "destino indisponível".to_string())
}

#[tauri::command]
pub async fn list_snapshots(
    state: State<'_, AppState>,
    destination_id: String,
) -> Res<Vec<GameSnapshotsView>> {
    let root = destination_root(&state, &destination_id)?;
    blocking(move || {
        let idx = LibraryIndex::load(&root).map_err(|e| e.to_string())?;
        let mut out: Vec<GameSnapshotsView> = idx
            .games
            .iter()
            .filter(|(_, refs)| !refs.is_empty())
            .map(|(id, refs)| {
                let latest = refs.iter().max_by_key(|r| r.created);
                let game_name = latest
                    .and_then(|r| load_meta(&root, id, &r.stem).ok())
                    .map(|m| m.game_name)
                    .unwrap_or_else(|| id.clone());
                let mut snapshots: Vec<SnapshotView> = refs
                    .iter()
                    .map(|r| SnapshotView {
                        stem: r.stem.clone(),
                        created: r.created.to_rfc3339(),
                        pinned: r.pinned,
                        total_bytes: r.total_bytes,
                    })
                    .collect();
                snapshots.sort_by(|a, b| b.created.cmp(&a.created));
                GameSnapshotsView {
                    game_id: id.clone(),
                    game_name,
                    snapshots,
                }
            })
            .collect();
        out.sort_by_key(|g| g.game_name.to_lowercase());
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn plan_restore(
    state: State<'_, AppState>,
    destination_id: String,
    game_id: String,
    stem: String,
) -> Res<RestorePlan> {
    let root = destination_root(&state, &destination_id)?;
    blocking(move || {
        let installed = detect_all();
        let ctx = current_ctx(&installed);
        let meta = load_meta(&root, &game_id, &stem).map_err(|e| e.to_string())?;
        core_plan(&root, &game_id, &stem, &ctx, &vars_for(&meta, &installed))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Executa a restauração. O plano é refeito aqui (não confia no que veio da interface).
#[tauri::command]
pub async fn apply_restore(
    state: State<'_, AppState>,
    destination_id: String,
    game_id: String,
    stem: String,
    overrides: BTreeMap<String, String>,
) -> Res<RestoreReport> {
    let root = destination_root(&state, &destination_id)?;
    blocking(move || {
        let installed = detect_all();
        let ctx = current_ctx(&installed);
        let meta = load_meta(&root, &game_id, &stem).map_err(|e| e.to_string())?;
        let plan = core_plan(&root, &game_id, &stem, &ctx, &vars_for(&meta, &installed))
            .map_err(|e| e.to_string())?;
        let overrides = resolve_overrides(&plan, &overrides).map_err(|e| e.to_string())?;
        core_apply(&plan, &root, &ctx, &overrides, Utc::now()).map_err(|e| e.to_string())
    })
    .await
}

/// Confirma (mantém) ou descarta um possível save da heurística.
#[tauri::command]
pub async fn confirm_heuristic(state: State<'_, AppState>, game_id: String, keep: bool) -> Res<()> {
    state.update_config(|c| {
        if keep {
            if !c.confirmed_games.contains(&game_id) {
                c.confirmed_games.push(game_id.clone());
            }
            c.ignored_games.retain(|g| g != &game_id);
        } else if !c.ignored_games.contains(&game_id) {
            c.ignored_games.push(game_id.clone());
        }
    })
}

#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
