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
            .filter_map(|d| match d.kind {
                // Na nuvem só conta a versão que já subiu.
                DestinationKind::Nuvem(_) => Some(crate::nuvem::indice_enviado(&d.root)),
                _ => LibraryIndex::load(&d.root).ok(),
            })
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
    let inicio = std::time::Instant::now();
    let result = core_scan(&ctx, &manifest, &installed, &opts);
    log::info!(
        "varredura: {} jogos ({} acima do limite) em {} ms",
        result.games.len(),
        result.over_limit.len(),
        inicio.elapsed().as_millis()
    );
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

/// Nomes dos processos abertos agora (para saber se o app da nuvem está rodando).
fn nomes_de_processos() -> Vec<String> {
    let Ok(mut sys) = sysinfo::System::new() else {
        return Vec::new();
    };
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .map(|p| p.name().to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
pub async fn list_destinations_cmd(state: State<'_, AppState>) -> Res<Vec<Destination>> {
    let cfg = state.config();
    blocking(move || {
        let mut d = list_destinations(&cfg);
        savesgg_core::destinations::marcar_clientes(&mut d, &nomes_de_processos());
        Ok(d)
    })
    .await
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

/// Destino configurado/detectado com esta raiz (para usar o mesmo id e a mesma trava).
fn destino_da_raiz(dests: &[Destination], root: &std::path::Path) -> Option<Destination> {
    let alvo = savesgg_core::paths::norm(root).to_lowercase();
    dests
        .iter()
        .find(|d| savesgg_core::paths::norm(&d.root).to_lowercase() == alvo)
        .cloned()
}

/// Abre o seletor e usa a pasta como origem de restauração só nesta sessão.
#[tauri::command]
pub async fn open_backup_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Res<Option<Destination>> {
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let picked = picked
        .into_path()
        .map_err(|_| "pasta inválida".to_string())?;
    let root = blocking(move || Ok(savesgg_core::destinations::resolve_backup_folder(&picked)))
        .await?
        .ok_or_else(|| "Nenhum backup do SavesGG nesta pasta".to_string())?;
    // Já é um destino conhecido: devolve ele, para a restauração e o motor automático
    // usarem a mesma trava (senão os dois poderiam gravar o índice ao mesmo tempo).
    let cfg = state.config();
    let raiz = root.clone();
    if let Some(d) = blocking(move || Ok(destino_da_raiz(&list_destinations(&cfg), &raiz))).await? {
        return Ok(Some(d));
    }
    {
        let mut abertas = state
            .pastas_abertas
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !abertas.contains(&root) {
            abertas.push(root.clone());
        }
    }
    Ok(Some(Destination {
        id: id_pasta_aberta(&root),
        label: "Pasta escolhida".into(),
        kind: savesgg_core::destinations::DestinationKind::PastaLocal,
        root,
        available: true,
        free_bytes: None,
        has_backup: true,
        cliente_ativo: None,
    }))
}

/// Abre o Explorer na pasta de logs do app.
#[tauri::command]
pub fn open_logs_folder(app: tauri::AppHandle) -> Res<()> {
    let dir = match crate::pasta_logs_portatil() {
        Some(d) => d,
        None => app
            .path()
            .app_log_dir()
            .map_err(|_| "pasta de logs indisponível".to_string())?,
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
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
    let app2 = app.clone();
    let r =
        blocking(move || backup_blocking(&app2.state::<AppState>(), &game_ids, &destination_ids))
            .await;
    app.state::<AppState>().enviador.avisar();
    r
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
    let app2 = app.clone();
    let r = blocking(move || {
        Ok(copy_all_blocking(
            &app2.state::<AppState>(),
            &destination_id,
        ))
    })
    .await;
    app.state::<AppState>().enviador.avisar();
    r
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
    /// Quantas versões guardar deste jogo, se diferente do padrão.
    pub keep: Option<usize>,
}

/// Confere se a versão existe no índice do destino (não confia na interface).
fn versao_existe(root: &std::path::Path, game_id: &str, stem: &str) -> Res<()> {
    let idx = LibraryIndex::load(root).map_err(|e| e.to_string())?;
    idx.games
        .get(game_id)
        .is_some_and(|v| v.iter().any(|r| r.stem == stem))
        .then_some(())
        .ok_or_else(|| "versão não encontrada neste backup".to_string())
}

/// Fixa ou solta uma versão (fixadas nunca são apagadas).
#[tauri::command]
pub async fn set_pinned_cmd(
    state: State<'_, AppState>,
    destination_id: String,
    game_id: String,
    stem: String,
    pinned: bool,
) -> Res<()> {
    let root = destination_root(&state, &destination_id)?;
    let lock = state.locks.lock(&destination_id);
    blocking(move || {
        let _trava = lock.lock().unwrap_or_else(|e| e.into_inner());
        versao_existe(&root, &game_id, &stem)?;
        savesgg_core::backup::set_pinned(&root, &game_id, &stem, pinned).map_err(|e| e.to_string())
    })
    .await
}

/// Quantas versões guardar por jogo (padrão de todos).
#[tauri::command]
pub fn set_keep_versions(state: State<'_, AppState>, n: usize) -> Res<()> {
    let n = savesgg_core::config::validar_keep(n)?;
    state.update_config(|c| c.keep_versions = n)
}

/// Quantas versões guardar de um jogo; `None` volta ao padrão.
#[tauri::command]
pub fn set_game_keep(state: State<'_, AppState>, game_id: String, n: Option<usize>) -> Res<()> {
    let n = n.map(savesgg_core::config::validar_keep).transpose()?;
    if game_id.is_empty() || game_id.len() > 200 {
        return Err("jogo inválido".into());
    }
    state.update_config(|c| match n {
        Some(n) => {
            c.per_game_keep.insert(game_id.clone(), n);
        }
        None => {
            c.per_game_keep.remove(&game_id);
        }
    })
}

/// Id de uma pasta aberta na tela Restaurar.
pub fn id_pasta_aberta(root: &std::path::Path) -> String {
    format!("aberta:{}", savesgg_core::paths::norm(root).to_lowercase())
}

fn destination_root(state: &AppState, destination_id: &str) -> Res<PathBuf> {
    if destination_id.starts_with("aberta:") {
        return state
            .pastas_abertas
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|p| id_pasta_aberta(p) == destination_id && p.is_dir())
            .cloned()
            .ok_or_else(|| "pasta indisponível".to_string());
    }
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
    let per_game = state.config().per_game_keep;
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
                    keep: per_game.get(id).copied(),
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
    let lock = state.locks.lock(&destination_id);
    let conta = crate::nuvem::conta_do_destino(&state.config(), &destination_id);
    blocking(move || {
        let _trava = lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = &conta {
            let p = crate::nuvem::provedor_de(c).map_err(|e| e.to_string())?;
            savesgg_core::nuvem::sync::garantir_zip(&root, p.as_ref(), &game_id, &stem)
                .map_err(|e| e.to_string())?;
        }
        let installed = detect_all();
        let ctx = current_ctx(&installed);
        let meta = load_meta(&root, &game_id, &stem).map_err(|e| e.to_string())?;
        let plan = core_plan(&root, &game_id, &stem, &ctx, &vars_for(&meta, &installed))
            .map_err(|e| e.to_string())?;
        let overrides = resolve_overrides(&plan, &overrides).map_err(|e| e.to_string())?;
        let r = core_apply(&plan, &root, &ctx, &overrides, Utc::now());
        match &r {
            Ok(rep) => log::info!(
                "restauração: {game_id} ({stem}) → {} arquivos",
                rep.restored
            ),
            Err(e) => log::error!("restauração: {game_id} ({stem}) falhou: {e}"),
        }
        r.map_err(|e| e.to_string())
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
    let conta = crate::nuvem::conta_do_destino(&state.config(), &destination_id);
    blocking(move || {
        let provedor = match &conta {
            Some(c) => Some(crate::nuvem::provedor_de(c).map_err(|e| e.to_string())?),
            None => None,
        };
        // Detecção e leitura do índice fora da trava; a trava é pega jogo a jogo, para o
        // motor automático e o "Fixar" não ficarem parados durante o lote inteiro.
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
                let _trava = lock.lock().unwrap_or_else(|e| e.into_inner());
                // Conta de nuvem: baixa o zip da versão que vai ser restaurada.
                let versao = idx
                    .games
                    .get(&id)
                    .and_then(|v| savesgg_core::backup::latest_restorable(v));
                let baixou = match (&provedor, versao) {
                    (Some(p), Some(r)) => {
                        savesgg_core::nuvem::sync::garantir_zip(&root, p.as_ref(), &id, &r.stem)
                            .map_err(|e| e.to_string())
                    }
                    _ => Ok(()),
                };
                match baixou {
                    Ok(()) => restore_latest(&root, &id, &ctx, &installed, Utc::now()),
                    Err(mensagem) => LoteResultado::Erro { mensagem },
                }
            } else {
                LoteResultado::Erro {
                    mensagem: "jogo não encontrado neste backup".into(),
                }
            };
            log::info!("restauração em lote: {id} → {resultado:?}");
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

#[derive(Debug, Serialize)]
pub struct NuvemDisponivel {
    pub gdrive: bool,
}

/// Quais logins na nuvem este build tem (os ids do cadastro vêm no build).
#[tauri::command]
pub fn login_nuvem_disponivel() -> NuvemDisponivel {
    NuvemDisponivel {
        gdrive: crate::oauth::cliente(savesgg_core::nuvem::ProvedorNuvem::Gdrive).is_some(),
    }
}

/// Baixa a lista de versões da nuvem para a preparação de uma conta.
fn atualizar_conta(state: &AppState, conta: &savesgg_core::nuvem::ContaNuvem) -> Res<usize> {
    let lock = state.locks.lock(&conta.destino_id());
    let _trava = lock.lock().unwrap_or_else(|e| e.into_inner());
    let prep = crate::nuvem::preparacao(conta);
    std::fs::create_dir_all(&prep).map_err(|e| e.to_string())?;
    let p = crate::nuvem::provedor_de(conta).map_err(|e| e.to_string())?;
    savesgg_core::nuvem::sync::atualizar_da_nuvem(&prep, p.as_ref()).map_err(|e| e.to_string())
}

/// Entra numa conta de nuvem pelo navegador. A conta vira destino (também do backup
/// automático) e as versões que já estão lá aparecem para restaurar.
#[tauri::command]
pub async fn entrar_nuvem(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    provedor: savesgg_core::nuvem::ProvedorNuvem,
) -> Res<Destination> {
    let conta =
        blocking(move || crate::oauth::entrar(provedor, crate::oauth::abrir_navegador)).await?;
    let id = conta.destino_id();
    state.update_config(|c| {
        c.contas_nuvem.retain(|x| x.destino_id() != id);
        c.contas_nuvem.push(conta.clone());
        if !c.auto_destinations.contains(&id) {
            c.auto_destinations.push(id.clone());
        }
    })?;
    let app2 = app.clone();
    let c2 = conta.clone();
    // A conta já está salva: se a primeira leitura da nuvem falhar (internet caiu),
    // o login vale mesmo assim e a tela Restaurar tenta de novo ao abrir.
    if let Err(e) = blocking(move || atualizar_conta(&app2.state::<AppState>(), &c2)).await {
        log::warn!("nuvem: conta conectada, mas a leitura da nuvem falhou: {e}");
    }
    state.enviador.avisar();
    savesgg_core::destinations::destinos_nuvem(
        std::slice::from_ref(&conta),
        &crate::nuvem::base_local(),
    )
    .into_iter()
    .next()
    .ok_or_else(|| "conta conectada, mas não apareceu como destino".to_string())
}

/// Para de esperar o login em andamento (a pessoa desistiu no navegador).
#[tauri::command]
pub fn cancelar_login() {
    crate::oauth::CANCELAR_LOGIN.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Sai de uma conta: apaga o login guardado e a pasta local. Os backups na nuvem ficam.
#[tauri::command]
pub async fn sair_nuvem(state: State<'_, AppState>, destination_id: String) -> Res<()> {
    let conta = crate::nuvem::conta_do_destino(&state.config(), &destination_id)
        .ok_or_else(|| "conta não encontrada".to_string())?;
    crate::oauth::sair(&destination_id);
    state.update_config(|c| {
        c.contas_nuvem.retain(|x| x.destino_id() != destination_id);
        c.auto_destinations.retain(|x| x != &destination_id);
    })?;
    let lock = state.locks.lock(&destination_id);
    let prep = crate::nuvem::preparacao(&conta);
    blocking(move || {
        let _trava = lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = std::fs::remove_dir_all(&prep);
        Ok(())
    })
    .await?;
    state.enviador.avisar();
    log::info!("nuvem: conta desconectada");
    Ok(())
}

/// Pede uma passada de envio agora.
#[tauri::command]
pub fn enviar_agora(state: State<'_, AppState>) {
    state.enviador.avisar();
}

/// Baixa da nuvem a lista de versões (antes de restaurar). Devolve quantas chegaram.
#[tauri::command]
pub async fn atualizar_da_nuvem_cmd(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    destination_id: String,
) -> Res<usize> {
    let conta = crate::nuvem::conta_do_destino(&state.config(), &destination_id)
        .ok_or_else(|| "conta não encontrada".to_string())?;
    blocking(move || atualizar_conta(&app.state::<AppState>(), &conta)).await
}

/// Acrescenta um jogo manual, recusando nome que gera o mesmo id de outro.
fn adicionar_manual(
    c: &mut savesgg_core::config::Config,
    m: savesgg_core::config::ManualGame,
) -> Res<()> {
    let id = savesgg_core::scan::game_id(&m.name);
    if c.manual_games
        .iter()
        .any(|g| savesgg_core::scan::game_id(&g.name) == id)
    {
        return Err("Já existe um jogo com esse nome".into());
    }
    c.manual_games.push(m);
    Ok(())
}

/// O nome gera o id de um jogo manual ou de um jogo já detectado na varredura?
/// (Ids iguais seriam trocados por `dedupe_ids` e o jogo detectado perderia o histórico.)
fn nome_ja_usado(
    nome: &str,
    manuais: &[savesgg_core::config::ManualGame],
    varredura: &[&savesgg_core::scan::DetectedGame],
) -> bool {
    let id = savesgg_core::scan::game_id(nome);
    manuais
        .iter()
        .any(|g| savesgg_core::scan::game_id(&g.name) == id)
        || varredura
            .iter()
            .any(|g| g.id == id || savesgg_core::scan::game_id(&g.name) == id)
}

/// Tira um jogo manual da configuração (os backups continuam). Procura pelo nome que
/// a varredura mostrou (o id pode ter sido trocado por `dedupe_ids`); sem ele, pelo id.
fn remover_manual(
    c: &mut savesgg_core::config::Config,
    game_id: &str,
    nome_na_varredura: Option<&str>,
) -> bool {
    let antes = c.manual_games.len();
    c.manual_games.retain(|g| match nome_na_varredura {
        Some(nome) => g.name != nome,
        None => savesgg_core::scan::game_id(&g.name) != game_id,
    });
    c.manual_games.len() != antes
}

/// Adiciona um jogo à mão: valida o nome, pede a pasta do save e, opcionalmente, o .exe.
/// `None` se o usuário cancelar a escolha da pasta.
#[tauri::command]
pub async fn add_manual_game(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Res<Option<Vec<GameView>>> {
    let name = savesgg_core::scan::validar_nome_jogo(&name)?;
    let usado = {
        let scan = state
            .last_scan
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let jogos: Vec<&savesgg_core::scan::DetectedGame> = scan
            .as_ref()
            .map(|s| s.result.games.iter().chain(&s.result.over_limit).collect())
            .unwrap_or_default();
        nome_ja_usado(&name, &state.config().manual_games, &jogos)
    };
    if usado {
        return Err("Já existe um jogo com esse nome".into());
    }
    let Some(pasta) = app
        .dialog()
        .file()
        .set_title("Pasta onde o jogo guarda os saves")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let save_dir = pasta
        .into_path()
        .map_err(|_| "pasta inválida".to_string())?;
    let exe = app
        .dialog()
        .file()
        .set_title("Executável do jogo (opcional)")
        .add_filter("Programa", &["exe"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok());
    let mut erro = None;
    state.update_config(|c| {
        // `adicionar_manual` confere de novo os outros jogos manuais (corrida entre cliques).
        erro = adicionar_manual(
            c,
            savesgg_core::config::ManualGame {
                name,
                save_dir,
                exe,
            },
        )
        .err();
    })?;
    if let Some(e) = erro {
        return Err(e);
    }
    let app2 = app.clone();
    blocking(move || run_scan_blocking(&app2.state::<AppState>()))
        .await
        .map(Some)
}

/// Remove um jogo adicionado à mão (os backups dele continuam guardados) e refaz a
/// varredura, para ele sumir da tela e do backup automático.
#[tauri::command]
pub async fn remove_manual_game(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    game_id: String,
) -> Res<Vec<GameView>> {
    let nome = state
        .last_scan
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|s| s.find(&game_id))
        .filter(|g| g.source == savesgg_core::scan::Source::Manual)
        .map(|g| g.name.clone());
    let mut achou = false;
    state.update_config(|c| achou = remover_manual(c, &game_id, nome.as_deref()))?;
    if !achou {
        return Err("jogo não encontrado".into());
    }
    let app2 = app.clone();
    blocking(move || run_scan_blocking(&app2.state::<AppState>())).await
}

#[derive(Debug, Serialize)]
pub struct IgnoredView {
    pub id: String,
    pub name: String,
}

/// Possíveis saves descartados, com o nome da última varredura (ou o id).
#[tauri::command]
pub fn ignored_games(state: State<'_, AppState>) -> Res<Vec<IgnoredView>> {
    let ids = state.config().ignored_games;
    let scan = state
        .last_scan
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Ok(ids
        .into_iter()
        .map(|id| IgnoredView {
            name: scan
                .as_ref()
                .and_then(|s| s.find(&id))
                .map(|g| g.name.clone())
                .unwrap_or_else(|| id.clone()),
            id,
        })
        .collect())
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

/// Página de downloads (fixa: nada vem da tela).
pub const PAGINA_DOWNLOAD: &str =
    "https://github.com/thelipesousa/savesgg-releases/releases/latest";

/// Rodando do pendrive (pasta `dados` ao lado do exe).
#[tauri::command]
pub fn modo_portatil() -> bool {
    savesgg_core::pastas::Pastas::atual().portatil
}

/// Abre a página de downloads no navegador (atualização da versão portable).
#[tauri::command]
pub fn abrir_pagina_download() -> Res<()> {
    std::process::Command::new("explorer")
        .arg(PAGINA_DOWNLOAD)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
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
    /// Rodando do pendrive: a tela esconde "Iniciar com o Windows".
    pub portatil: bool,
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
        portatil: savesgg_core::pastas::Pastas::atual().portatil,
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

    fn manual(nome: &str) -> savesgg_core::config::ManualGame {
        savesgg_core::config::ManualGame {
            name: nome.into(),
            save_dir: "D:/Jogos/X".into(),
            exe: None,
        }
    }

    #[test]
    fn adicionar_manual_recusa_nome_repetido() {
        let mut c = savesgg_core::config::Config::default();
        adicionar_manual(&mut c, manual("Meu Jogo")).unwrap();
        assert!(adicionar_manual(&mut c, manual("meu jogo!")).is_err());
        assert_eq!(c.manual_games.len(), 1);
    }

    #[test]
    fn remover_manual_tira_da_config() {
        let mut c = savesgg_core::config::Config::default();
        adicionar_manual(&mut c, manual("Meu Jogo")).unwrap();
        adicionar_manual(&mut c, manual("Outro")).unwrap();
        assert!(remover_manual(&mut c, "meu-jogo", None));
        assert_eq!(c.manual_games.len(), 1);
        assert_eq!(c.manual_games[0].name, "Outro");
        assert!(!remover_manual(&mut c, "meu-jogo", None));
    }

    #[test]
    fn remover_manual_pelo_nome_da_varredura_mesmo_com_id_trocado() {
        // dedupe_ids pode trocar o id ("hades" vira "hades-1a2b"); o nome continua o mesmo.
        let mut c = savesgg_core::config::Config::default();
        adicionar_manual(&mut c, manual("Hades")).unwrap();
        assert!(remover_manual(&mut c, "hades-1a2b", Some("Hades")));
        assert!(c.manual_games.is_empty());
    }

    #[test]
    fn nome_manual_igual_a_jogo_detectado_e_recusado() {
        let c = savesgg_core::config::Config::default();
        let mut detectado = jogo("hades");
        detectado.name = "Hades".into();
        assert!(nome_ja_usado("hades", &c.manual_games, &[&detectado]));
        assert!(!nome_ja_usado("Celeste", &c.manual_games, &[&detectado]));
    }

    #[test]
    fn pasta_aberta_que_ja_e_destino_usa_o_id_do_destino() {
        let dests = savesgg_core::destinations::build_destinations(
            &[],
            &[],
            &[PathBuf::from("D:/Backups")],
        );
        let achado = destino_da_raiz(&dests, std::path::Path::new("d:/BACKUPS/")).unwrap();
        assert_eq!(achado.id, dests[0].id);
        assert!(destino_da_raiz(&dests, std::path::Path::new("E:/Outra")).is_none());
    }

    #[test]
    fn fixar_versao_inexistente_falha() {
        let t = tempfile::tempdir().unwrap();
        let idx = LibraryIndex::default();
        idx.save_atomic(t.path()).unwrap();
        assert!(versao_existe(t.path(), "hades", "2026-11-20_10-00-00").is_err());
    }

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
            last_modified: None,
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
