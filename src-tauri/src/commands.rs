//! Comandos chamados pela interface. Finos: a lógica fica em `savesgg-core`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use savesgg_core::backup::{BackupOutcome, LibraryIndex, backup_game, load_meta};
use savesgg_core::destinations::{
    Destination, DestinationKind, counts_as_off_pc, list_destinations,
};
use savesgg_core::protection::{BackupHistory, assess, sort_for_display};
use savesgg_core::restore::{
    LoteResultado, RestorePlan, RestoreReport, apply_restore as core_apply,
    plan_restore as core_plan, resolve_overrides, restore_latest, vars_for,
};
use savesgg_core::scan::{ScanOptions, find_emulator_roots, scan as core_scan};
use savesgg_core::stores::detect_all;
use savesgg_core::sync_folders::detect_sync_roots;

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
    let (views, risk_ids) = make_views(
        &result,
        &ctx,
        &cfg,
        &history(&dests),
        &detect_sync_roots(),
        Utc::now(),
    );
    let at_risk = risk_ids.len();
    *state.at_risk_ids.lock().unwrap_or_else(|e| e.into_inner()) = risk_ids;
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

/// Monta a lista da tela a partir de uma varredura, com a configuração (jogos
/// descartados/confirmados) e o histórico de backups de agora. Devolve também os
/// jogos sem cópia fora do PC.
pub fn make_views(
    result: &savesgg_core::scan::ScanResult,
    ctx: &savesgg_core::paths::PathContext,
    cfg: &savesgg_core::config::Config,
    hist: &dyn BackupHistory,
    sync: &[savesgg_core::sync_folders::SyncRoot],
    now: DateTime<Utc>,
) -> (Vec<GameView>, std::collections::BTreeSet<String>) {
    let visible = |g: &&savesgg_core::scan::DetectedGame| !cfg.ignored_games.contains(&g.id);
    let mut pairs: Vec<_> = result
        .games
        .iter()
        .filter(visible)
        .map(|g| (g.clone(), assess(g, sync, hist, now)))
        .collect();
    sort_for_display(&mut pairs);
    let mut views: Vec<GameView> = pairs
        .iter()
        .map(|(g, a)| {
            game_view(
                g,
                a,
                ctx,
                hist.last_backup(&g.id),
                false,
                cfg.confirmed_games.contains(&g.id),
            )
        })
        .collect();
    for g in result.over_limit.iter().filter(visible) {
        let a = assess(g, sync, hist, now);
        views.push(game_view(
            g,
            &a,
            ctx,
            hist.last_backup(&g.id),
            true,
            cfg.confirmed_games.contains(&g.id),
        ));
    }
    let risk = views
        .iter()
        .filter(|v| !v.grande && v.priority == savesgg_core::protection::Priority::Alta)
        .map(|v| v.id.clone())
        .collect();
    (views, risk)
}

/// Lista da última varredura (a do motor ao abrir o app), sem escanear o disco de novo,
/// mas recalculada com a configuração e os backups de agora (descartes, backups feitos).
#[tauri::command]
pub async fn cached_scan(app: tauri::AppHandle) -> Res<Option<Vec<GameView>>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let Some(data) = state
            .last_scan
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return Ok(None);
        };
        let cfg = state.config();
        let dests = list_destinations(&cfg);
        let (views, _) = make_views(
            &data.result,
            &data.ctx,
            &cfg,
            &history(&dests),
            &detect_sync_roots(),
            Utc::now(),
        );
        Ok(Some(views))
    })
    .await
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

fn find_destinations(cfg: &savesgg_core::config::Config, ids: &[String]) -> Vec<Destination> {
    list_destinations(cfg)
        .into_iter()
        .filter(|d| ids.contains(&d.id))
        .collect()
}

/// Backup manual de vários jogos para vários destinos, com a trava de cada destino.
pub fn backup_blocking(
    state: &AppState,
    game_ids: &[String],
    destination_ids: &[String],
) -> Res<Vec<BackupResultView>> {
    let data = state
        .last_scan
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("faça uma varredura primeiro")?;
    let cfg = state.config();
    let dests = find_destinations(&cfg, destination_ids);
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
        for id in game_ids {
            let Some(g) = data.find(id) else { continue };
            // Trava por jogo, não pelo destino inteiro: copiar 30 GB não pode parar
            // o motor automático por minutos.
            let lock = state.locks.lock(&d.id);
            let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
            let g = savesgg_core::scan::refresh_files(g);
            let r = backup_game(&g, &data.ctx, &d.root, cfg.keep_for(id), Utc::now());
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
}

#[tauri::command]
pub async fn backup_now(
    app: tauri::AppHandle,
    game_ids: Vec<String>,
    destination_ids: Vec<String>,
) -> Res<Vec<BackupResultView>> {
    blocking(move || backup_blocking(&app.state::<AppState>(), &game_ids, &destination_ids)).await
}

/// Jogos que entram em "copiar tudo": sem os ignorados e sem possíveis saves ou
/// grandes demais que o usuário ainda não confirmou.
pub fn all_backup_ids(data: &ScanData, cfg: &savesgg_core::config::Config) -> Vec<String> {
    data.result
        .games
        .iter()
        .chain(&data.result.over_limit)
        .filter(|g| !cfg.ignored_games.contains(&g.id))
        .filter(|g| {
            let grande = data.result.over_limit.iter().any(|o| o.id == g.id);
            let precisa_confirmar = g.source == savesgg_core::scan::Source::Heuristica || grande;
            !precisa_confirmar || cfg.confirmed_games.contains(&g.id)
        })
        .map(|g| g.id.clone())
        .collect()
}

/// Copia todos os jogos para um destino (aviso de pendrive/HD e menu da bandeja).
pub fn copy_all_blocking(state: &AppState, destination_id: &str) -> Vec<BackupResultView> {
    let Some(data) = state
        .last_scan
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    else {
        return Vec::new();
    };
    let ids = all_backup_ids(&data, &state.config());
    backup_blocking(state, &ids, &[destination_id.to_string()]).unwrap_or_default()
}

/// Copia todos os jogos para um destino (aviso de disco plugado).
#[tauri::command]
pub async fn copy_all_to(
    app: tauri::AppHandle,
    destination_id: String,
) -> Res<Vec<BackupResultView>> {
    blocking(move || Ok(copy_all_blocking(&app.state::<AppState>(), &destination_id))).await
}

#[derive(Debug, Serialize)]
pub struct SnapshotView {
    pub stem: String,
    pub created: String,
    pub pinned: bool,
    pub total_bytes: u64,
    pub seguranca: bool,
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
                        seguranca: r.seguranca,
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

#[derive(Debug, Serialize)]
pub struct LoteItemView {
    pub game_id: String,
    pub game_name: String,
    pub resultado: LoteResultado,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressoRestauracao {
    pub feitos: usize,
    pub total: usize,
    pub jogo: String,
}

/// Restaura a versão mais recente de vários jogos. Um erro num jogo não para os outros.
#[tauri::command]
pub async fn restore_many(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    destination_id: String,
    game_ids: Vec<String>,
) -> Res<Vec<LoteItemView>> {
    let root = destination_root(&state, &destination_id)?;
    let lock = state.locks.lock(&destination_id);
    blocking(move || {
        let _trava = lock.lock().unwrap_or_else(|e| e.into_inner());
        let idx = LibraryIndex::load(&root).map_err(|e| e.to_string())?;
        let installed = detect_all();
        let ctx = current_ctx(&installed);
        let total = game_ids.len();
        let mut out = Vec::with_capacity(total);
        for (feitos, id) in game_ids.into_iter().enumerate() {
            let game_name = idx
                .games
                .get(&id)
                .and_then(|refs| refs.iter().max_by_key(|r| r.created))
                .and_then(|r| load_meta(&root, &id, &r.stem).ok())
                .map(|m| m.game_name)
                .unwrap_or_else(|| id.clone());
            let _ = app.emit(
                "restauracao-progresso",
                ProgressoRestauracao {
                    feitos,
                    total,
                    jogo: game_name.clone(),
                },
            );
            let resultado = if idx.games.contains_key(&id) {
                restore_latest(&root, &id, &ctx, &installed, Utc::now())
            } else {
                LoteResultado::Erro {
                    mensagem: "jogo não encontrado neste backup".into(),
                }
            };
            out.push(LoteItemView {
                game_id: id,
                game_name,
                resultado,
            });
        }
        Ok(out)
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

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub auto_destinations: Vec<String>,
    pub autostart: bool,
    pub auto_paused: bool,
    pub keep_versions: usize,
    pub theme: String,
    pub game_view: String,
    pub wizard_done: bool,
}

/// Aceita só valores de uma lista fechada (tema, exibição).
pub fn validate_choice(value: &str, allowed: &[&str]) -> Res<String> {
    allowed
        .iter()
        .find(|a| **a == value)
        .map(|a| a.to_string())
        .ok_or_else(|| format!("opção inválida: {value}"))
}

#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: String) -> Res<()> {
    let t = validate_choice(&theme, &savesgg_core::config::TEMAS)?;
    state.update_config(|c| c.theme = t)
}

#[tauri::command]
pub fn set_game_view(state: State<'_, AppState>, view: String) -> Res<()> {
    let v = validate_choice(&view, &savesgg_core::config::EXIBICOES)?;
    state.update_config(|c| c.game_view = v)
}

#[tauri::command]
pub fn complete_wizard(state: State<'_, AppState>) -> Res<()> {
    state.update_config(|c| c.wizard_done = true)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsView {
    let c = state.config();
    SettingsView {
        auto_destinations: c.auto_destinations.clone(),
        autostart: c.autostart,
        auto_paused: c.auto_paused,
        keep_versions: c.keep_versions,
        theme: c.theme_or_default().to_string(),
        game_view: c.game_view_or_default().to_string(),
        wizard_done: c.wizard_done,
    }
}

/// Destinos do backup automático. Só aceita ids de destinos que o app conhece
/// (ou que já estavam escolhidos, como um pendrive desplugado agora).
#[tauri::command]
pub async fn set_auto_destinations(app: tauri::AppHandle, ids: Vec<String>) -> Res<()> {
    blocking(move || {
        let state = app.state::<AppState>();
        let cfg = state.config();
        let known = list_destinations(&cfg);
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| known.iter().any(|d| &d.id == id) || cfg.auto_destinations.contains(id))
            .collect();
        state.update_config(|c| c.auto_destinations = ids)
    })
    .await
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Res<()> {
    app.state::<AppState>()
        .update_config(|c| c.autostart = enabled)?;
    crate::sync_autostart(&app);
    Ok(())
}

#[tauri::command]
pub fn set_paused(app: tauri::AppHandle, paused: bool) -> Res<()> {
    app.state::<AppState>()
        .update_config(|c| c.auto_paused = paused)?;
    crate::auto::publish(&app, |s| s.paused = paused);
    Ok(())
}

#[tauri::command]
pub fn auto_status(state: State<'_, AppState>) -> crate::auto::AutoStatusView {
    state
        .auto_status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Hist(Option<DateTime<Utc>>);
    impl BackupHistory for Hist {
        fn last_backup(&self, id: &str) -> Option<DateTime<Utc>> {
            if id == "a" { self.0 } else { None }
        }
    }

    fn jogo(id: &str) -> savesgg_core::scan::DetectedGame {
        savesgg_core::scan::DetectedGame {
            id: id.into(),
            name: id.into(),
            store: savesgg_core::manifest::Store::SemLoja,
            save_roots: vec![],
            files: vec![],
            total_bytes: 0,
            cloud: Default::default(),
            source: savesgg_core::scan::Source::Manifesto,
            exe_hint: None,
            base: None,
            root: None,
            store_game_id: None,
            patterns: vec![],
            steam_app_id: None,
        }
    }

    #[test]
    fn lista_reflete_config_e_backups_de_agora() {
        let result = savesgg_core::scan::ScanResult {
            games: vec![jogo("a"), jogo("b"), jogo("descartado")],
            over_limit: vec![],
        };
        let cfg = savesgg_core::config::Config {
            ignored_games: vec!["descartado".into()],
            ..Default::default()
        };
        let ctx = savesgg_core::paths::PathContext::from_system();
        let agora = Utc::now();
        let (views, risco) = make_views(&result, &ctx, &cfg, &Hist(Some(agora)), &[], agora);
        let ids: Vec<_> = views.iter().map(|v| v.id.as_str()).collect();
        assert!(!ids.contains(&"descartado"), "{ids:?}");
        let a = views.iter().find(|v| v.id == "a").unwrap();
        assert!(a.last_backup.is_some());
        assert_eq!(a.priority, savesgg_core::protection::Priority::Baixa);
        assert!(!risco.contains("a") && risco.contains("b"));
    }

    #[test]
    fn valida_tema_e_exibicao() {
        use savesgg_core::config::{EXIBICOES, TEMAS};
        assert!(validate_choice("pastel", &TEMAS).is_ok());
        assert!(validate_choice("azul", &TEMAS).is_err());
        assert!(validate_choice("grade", &EXIBICOES).is_ok());
        assert!(validate_choice("", &EXIBICOES).is_err());
    }
}
