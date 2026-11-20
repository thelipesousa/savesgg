//! Restauração: devolve cada save ao lugar certo, no PC atual.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::CoreError;
use crate::backup::{
    BackupOutcome, LibraryIndex, SnapshotRef, backup_game, load_meta, set_pinned, snapshot_paths,
};
use crate::paths::expand;
pub use crate::paths::{GameVars, PathContext};
use crate::scan::{DetectedGame, Source};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreItem {
    pub zip_path: String,
    pub template: String,
    /// `None` = não deu para resolver neste PC (o usuário escolhe a pasta).
    pub target: Option<PathBuf>,
    pub exists: bool,
    pub existing_modified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestorePlan {
    pub game_id: String,
    pub game_name: String,
    pub stem: String,
    pub items: Vec<RestoreItem>,
    pub unresolved: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    pub restored: usize,
    pub safety_snapshot: Option<SnapshotRef>,
}

fn inseguro(s: &str) -> CoreError {
    CoreError::CaminhoInseguro(s.to_string())
}

/// Caminho dentro do zip: relativo, sem `..`, sem unidade.
fn check_zip_path(p: &str) -> Result<(), CoreError> {
    let bad = p.is_empty()
        || p.starts_with('/')
        || p.starts_with('\\')
        || p.contains(':')
        || p.split(['/', '\\']).any(|seg| seg == "..");
    if bad { Err(inseguro(p)) } else { Ok(()) }
}

/// Template sem `..` (não pode escapar da pasta do marcador).
fn check_template(t: &str) -> Result<(), CoreError> {
    if t.split(['/', '\\']).any(|seg| seg == "..") {
        Err(inseguro(t))
    } else {
        Ok(())
    }
}

/// A unidade do caminho existe neste PC?
fn drive_exists(p: &Path) -> bool {
    let s = p.to_string_lossy().replace('\\', "/");
    match s.split_once('/') {
        Some((drive, _)) if !drive.is_empty() => Path::new(&format!("{drive}/")).exists(),
        _ => false,
    }
}

fn modified(p: &Path) -> Option<DateTime<Utc>> {
    p.metadata()
        .ok()?
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
}

/// Monta o plano: para onde vai cada arquivo e o que já existe lá.
pub fn plan_restore(
    dest_root: &Path,
    game_id: &str,
    stem: &str,
    ctx: &PathContext,
    vars: &GameVars,
) -> Result<RestorePlan, CoreError> {
    let meta = load_meta(dest_root, game_id, stem)?;
    let mut c = ctx.clone();
    if c.store_user_id.is_none() {
        c.store_user_id = meta.store_user_id.clone();
    }
    let mut items = Vec::new();
    for f in &meta.files {
        check_zip_path(&f.zip_path)?;
        check_template(&f.template)?;
        let target = expand(&f.template, &c, vars)
            .filter(|s| !s.contains(['*', '?']))
            .map(PathBuf::from)
            .filter(|p| drive_exists(p));
        let exists = target.as_deref().is_some_and(Path::is_file);
        items.push(RestoreItem {
            zip_path: f.zip_path.clone(),
            template: f.template.clone(),
            existing_modified: if exists {
                target.as_deref().and_then(modified)
            } else {
                None
            },
            exists,
            target,
        });
    }
    Ok(RestorePlan {
        game_id: game_id.to_string(),
        game_name: meta.game_name,
        stem: stem.to_string(),
        unresolved: items.iter().filter(|i| i.target.is_none()).count(),
        conflicts: items.iter().filter(|i| i.exists).count(),
        items,
    })
}

/// Executa o plano. `overrides` (zip_path → arquivo final) resolve itens sem destino.
/// Antes de sobrescrever, guarda os saves atuais como uma versão fixada.
pub fn apply_restore(
    plan: &RestorePlan,
    dest_root: &Path,
    ctx: &PathContext,
    overrides: &BTreeMap<String, PathBuf>,
    now: DateTime<Utc>,
) -> Result<RestoreReport, CoreError> {
    let meta = load_meta(dest_root, &plan.game_id, &plan.stem)?;
    let (zip_file, _) = snapshot_paths(dest_root, &plan.game_id, &plan.stem);
    let mut zip = zip::ZipArchive::new(fs::File::open(&zip_file)?)
        .map_err(|e| CoreError::Formato(e.to_string()))?;

    // 1. Lê e confere tudo antes de escrever qualquer coisa.
    let mut writes: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for item in &plan.items {
        check_zip_path(&item.zip_path)?;
        let Some(target) = overrides.get(&item.zip_path).or(item.target.as_ref()) else {
            continue;
        };
        let expected = meta
            .files
            .iter()
            .find(|f| f.zip_path == item.zip_path)
            .ok_or_else(|| inseguro(&item.zip_path))?;
        let mut entry = zip
            .by_name(&item.zip_path)
            .map_err(|e| CoreError::Formato(e.to_string()))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        if blake3::hash(&bytes).to_hex().as_str() != expected.blake3 {
            return Err(CoreError::Formato(format!(
                "conteúdo de {} não confere com o backup",
                item.zip_path
            )));
        }
        writes.push((target.clone(), bytes));
    }

    // 2. Backup de segurança do que vai ser sobrescrito.
    let existing: Vec<PathBuf> = writes
        .iter()
        .map(|(t, _)| t.clone())
        .filter(|t| t.is_file())
        .collect();
    let mut safety_snapshot = None;
    if !existing.is_empty() {
        let game = DetectedGame {
            id: plan.game_id.clone(),
            name: meta.game_name.clone(),
            store: meta.store,
            save_roots: existing
                .iter()
                .filter_map(|p| p.parent().map(Path::to_path_buf))
                .collect(),
            total_bytes: 0,
            files: existing,
            cloud: Default::default(),
            source: Source::Manifesto,
            exe_hint: None,
        };
        let r = match backup_game(&game, ctx, dest_root, usize::MAX, now)? {
            BackupOutcome::Criado(r) => r,
            BackupOutcome::SemMudanca => LibraryIndex::load(dest_root)?
                .games
                .get(&plan.game_id)
                .and_then(|v| v.iter().max_by_key(|r| r.created).cloned())
                .ok_or_else(|| CoreError::Formato("versão de segurança sumiu".into()))?,
        };
        set_pinned(dest_root, &plan.game_id, &r.stem, true)?;
        safety_snapshot = Some(SnapshotRef { pinned: true, ..r });
    }

    // 3. Escreve.
    for (target, bytes) in &writes {
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir)?;
        }
        crate::backup_write_atomic(target, bytes)?;
    }
    Ok(RestoreReport {
        restored: writes.len(),
        safety_snapshot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{BackupOutcome, LibraryIndex, backup_game, load_meta, snapshot_paths};
    use crate::manifest::{CloudFlags, Store};
    use crate::scan::{DetectedGame, Source};
    use chrono::{Duration, TimeZone, Utc};
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn ctx(root: &Path, user: &str, sid: &str) -> PathContext {
        let home = root.join("Users").join(user);
        PathContext {
            app_data: home.join("AppData/Roaming"),
            local_app_data: home.join("AppData/Local"),
            local_app_data_low: home.join("AppData/LocalLow"),
            documents: home.join("Documents"),
            public: root.join("Users/Public"),
            program_data: root.join("ProgramData"),
            os_user_name: user.into(),
            store_user_id: Some(sid.into()),
            home,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 10, 0, 0).unwrap()
    }

    /// Faz um backup do "Elden Ring" no PC da Ana e devolve (tempdir, destino, stem).
    fn backup_da_ana() -> (tempfile::TempDir, PathBuf, String) {
        let t = tempfile::tempdir().unwrap();
        let a = ctx(t.path(), "Ana", "111");
        let save = a.app_data.join("EldenRing/111");
        fs::create_dir_all(&save).unwrap();
        fs::write(save.join("ER0000.sl2"), b"progresso da ana").unwrap();
        let dest = t.path().join("Pendrive");
        fs::create_dir_all(&dest).unwrap();
        let game = DetectedGame {
            id: "elden-ring".into(),
            name: "Elden Ring".into(),
            store: Store::Steam,
            save_roots: vec![save.clone()],
            files: vec![save.join("ER0000.sl2")],
            total_bytes: 16,
            cloud: CloudFlags::default(),
            source: Source::Manifesto,
            exe_hint: None,
        };
        let BackupOutcome::Criado(r) = backup_game(&game, &a, &dest, 10, now()).unwrap() else {
            panic!()
        };
        (t, dest, r.stem)
    }

    #[test]
    fn restaura_em_pc_novo() {
        let (t, dest, stem) = backup_da_ana();
        let b = ctx(t.path(), "Bruno", "222");
        let plan = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        assert_eq!(plan.unresolved, 0);
        assert_eq!(plan.conflicts, 0);
        let rep = apply_restore(&plan, &dest, &b, &BTreeMap::new(), now()).unwrap();
        assert_eq!(rep.restored, 1);
        assert!(rep.safety_snapshot.is_none());
        let alvo = b.app_data.join("EldenRing/222/ER0000.sl2");
        assert_eq!(fs::read(alvo).unwrap(), b"progresso da ana");
    }

    #[test]
    fn caminho_absoluto_em_disco_ausente_fica_nao_resolvido() {
        let (t, dest, stem) = backup_da_ana();
        let (_, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.files[0].template = "Q:/Jogos/Elden/ER0000.sl2".into();
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        let plan = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        assert_eq!(plan.unresolved, 1);
        assert!(plan.items[0].target.is_none());
    }

    #[test]
    fn override_resolve_item_nao_resolvido() {
        let (t, dest, stem) = backup_da_ana();
        let (_, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.files[0].template = "Q:/Jogos/Elden/ER0000.sl2".into();
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        let plan = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        let escolhido = t.path().join("Escolhida/ER0000.sl2");
        let ov = BTreeMap::from([(plan.items[0].zip_path.clone(), escolhido.clone())]);
        let rep = apply_restore(&plan, &dest, &b, &ov, now()).unwrap();
        assert_eq!(rep.restored, 1);
        assert_eq!(fs::read(escolhido).unwrap(), b"progresso da ana");
    }

    #[test]
    fn conflito_cria_backup_fixado() {
        let (t, dest, stem) = backup_da_ana();
        let b = ctx(t.path(), "Bruno", "222");
        let alvo = b.app_data.join("EldenRing/222/ER0000.sl2");
        fs::create_dir_all(alvo.parent().unwrap()).unwrap();
        fs::write(&alvo, b"progresso do bruno").unwrap();
        let plan = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        assert_eq!(plan.conflicts, 1);
        assert!(plan.items[0].existing_modified.is_some());
        let rep = apply_restore(
            &plan,
            &dest,
            &b,
            &BTreeMap::new(),
            now() + Duration::hours(1),
        )
        .unwrap();
        let safety = rep.safety_snapshot.expect("backup de segurança");
        let idx = LibraryIndex::load(&dest).unwrap();
        let r = idx.games["elden-ring"]
            .iter()
            .find(|r| r.stem == safety.stem)
            .unwrap();
        assert!(r.pinned);
        assert_eq!(fs::read(&alvo).unwrap(), b"progresso da ana");
        let meta = load_meta(&dest, "elden-ring", &safety.stem).unwrap();
        assert_eq!(meta.files.len(), 1);
    }

    #[test]
    fn zip_slip_rejeitado() {
        let (t, dest, stem) = backup_da_ana();
        let (zip_path, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut zw = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        zw.start_file("../../evil.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(b"mal").unwrap();
        zw.finish().unwrap();
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.files[0].zip_path = "../../evil.txt".into();
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        let err = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap_err();
        assert!(matches!(err, CoreError::CaminhoInseguro(_)), "{err:?}");
        assert!(!t.path().join("evil.txt").exists());
    }

    #[test]
    fn template_com_ponto_ponto_rejeitado() {
        let (t, dest, stem) = backup_da_ana();
        let (_, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.files[0].template = "<winAppData>/../../../Windows/x.dll".into();
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        let err = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap_err();
        assert!(matches!(err, CoreError::CaminhoInseguro(_)), "{err:?}");
    }

    #[test]
    fn conteudo_adulterado_e_erro() {
        let (t, dest, stem) = backup_da_ana();
        let (_, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.files[0].blake3 = "0".repeat(64);
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        let plan = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        assert!(apply_restore(&plan, &dest, &b, &BTreeMap::new(), now()).is_err());
        assert!(!b.app_data.join("EldenRing/222/ER0000.sl2").exists());
    }
}
