//! Restauração: devolve cada save ao lugar certo, no PC atual.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::CoreError;
use crate::backup::SnapshotMeta;
use crate::backup::{
    BackupOutcome, LibraryIndex, SnapshotRef, backup_game, latest_restorable, load_meta,
    set_pinned, set_safety, snapshot_paths,
};
use crate::paths::expand;
pub use crate::paths::{GameVars, PathContext};
use crate::scan::{DetectedGame, Source};
use crate::stores::InstalledGame;

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

/// `<base>`/`<root>` do jogo no PC atual: o mesmo jogo (loja + ID) instalado aqui.
/// Sem correspondência, os arquivos de `<base>`/`<root>` ficam como não resolvidos.
pub fn vars_for(meta: &SnapshotMeta, installed: &[InstalledGame]) -> GameVars {
    let Some(id) = meta.store_game_id.as_deref() else {
        return GameVars::default();
    };
    installed
        .iter()
        .find(|i| i.store == meta.store && i.store_game_id.as_deref() == Some(id))
        .map(|i| GameVars {
            base: Some(i.install_dir.clone()),
            root: i.store_root.clone(),
            game: Some(i.name.clone()),
            store_game_id: Some(id.to_string()),
        })
        .unwrap_or_default()
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
            .filter(|p| drive_exists(p))
            // Caminho absoluto (fora das pastas conhecidas): só vale se a pasta já existe
            // aqui; senão é uma pasta fantasma que o jogo não lê, e o usuário escolhe.
            .filter(|p| f.template.starts_with('<') || p.parent().is_some_and(Path::is_dir));
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

/// Valida as pastas escolhidas pelo usuário para itens sem destino (zip_path → pasta) e
/// devolve zip_path → arquivo final. Recusa pasta relativa/inexistente e dois arquivos
/// indo para o mesmo lugar; ignora override de item que já tem destino.
pub fn resolve_overrides(
    plan: &RestorePlan,
    folders: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, PathBuf>, CoreError> {
    let mut out = BTreeMap::new();
    for item in plan.items.iter().filter(|i| i.target.is_none()) {
        let Some(folder) = folders.get(&item.zip_path) else {
            continue;
        };
        let dir = PathBuf::from(folder);
        if !dir.is_absolute() || !dir.is_dir() {
            return Err(inseguro(folder));
        }
        let name = Path::new(&item.zip_path)
            .file_name()
            .ok_or_else(|| inseguro(&item.zip_path))?;
        out.insert(item.zip_path.clone(), dir.join(name));
    }
    let mut todos: Vec<String> = plan
        .items
        .iter()
        .filter_map(|i| i.target.as_ref())
        .chain(out.values())
        .map(|p| crate::paths::norm(p).to_lowercase())
        .collect();
    let n = todos.len();
    todos.sort();
    todos.dedup();
    if todos.len() != n {
        return Err(CoreError::Formato(
            "dois arquivos iriam para o mesmo lugar; escolha pastas diferentes".into(),
        ));
    }
    Ok(out)
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

    // 2. Todo destino existente precisa aceitar escrita (jogo aberto costuma deixar
    //    ler mas não escrever): melhor falhar antes do que restaurar pela metade.
    for (target, _) in &writes {
        if target.is_file()
            && let Err(e) = fs::OpenOptions::new().write(true).open(target)
        {
            return Err(if matches!(e.raw_os_error(), Some(32) | Some(33)) {
                CoreError::ArquivoTravado(target.clone())
            } else {
                e.into()
            });
        }
    }

    // 3. Backup de segurança do que vai ser sobrescrito.
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
            base: None,
            root: None,
            store_game_id: None,
            patterns: Vec::new(),
            steam_app_id: None,
            last_modified: None,
        };
        // `SemMudanca`: o que está no PC já é igual à versão mais recente guardada, então
        // não há o que proteger e nenhuma versão normal é marcada como de segurança.
        if let BackupOutcome::Criado(r) = backup_game(&game, ctx, dest_root, usize::MAX, now)? {
            set_pinned(dest_root, &plan.game_id, &r.stem, true)?;
            set_safety(dest_root, &plan.game_id, &r.stem)?;
            safety_snapshot = Some(SnapshotRef {
                pinned: true,
                seguranca: true,
                ..r
            });
        }
    }

    // 4. Escreve. Se algo falhar no meio, diz qual versão guarda o estado anterior.
    for (i, (target, bytes)) in writes.iter().enumerate() {
        let r = target
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .map_err(CoreError::from)
            .and_then(|_| crate::backup_write_atomic(target, bytes));
        if let Err(e) = r {
            if i == 0 {
                return Err(e);
            }
            let seg = safety_snapshot
                .as_ref()
                .map_or("nenhuma".to_string(), |s| s.stem.clone());
            return Err(CoreError::Formato(format!(
                "restauração parcial ({i} de {} arquivos); a versão de segurança é {seg}: {e}",
                writes.len()
            )));
        }
    }
    Ok(RestoreReport {
        restored: writes.len(),
        safety_snapshot,
    })
}

/// Resultado da restauração de um jogo dentro de um lote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum LoteResultado {
    /// `seguranca`: versão criada com o que estava no PC antes (se havia algo).
    Restaurado {
        arquivos: usize,
        seguranca: Option<String>,
    },
    /// Algum arquivo não tem destino neste PC; o usuário escolhe a pasta pelo fluxo de um jogo.
    PrecisaDePasta {
        faltando: usize,
    },
    Erro {
        mensagem: String,
    },
}

/// Restaura a versão mais recente (que não seja de segurança) de um jogo, só se todos os
/// arquivos tiverem destino resolvido. Erros viram `Erro`, para o lote seguir.
pub fn restore_latest(
    dest_root: &Path,
    game_id: &str,
    ctx: &PathContext,
    installed: &[InstalledGame],
    now: DateTime<Utc>,
) -> LoteResultado {
    let erro = |e: CoreError| LoteResultado::Erro {
        mensagem: e.to_string(),
    };
    let idx = match LibraryIndex::load(dest_root) {
        Ok(i) => i,
        Err(e) => return erro(e),
    };
    let Some(refs) = idx.games.get(game_id).filter(|v| !v.is_empty()) else {
        return LoteResultado::Erro {
            mensagem: "jogo não encontrado neste backup".into(),
        };
    };
    let Some(r) = latest_restorable(refs) else {
        return LoteResultado::Erro {
            mensagem: "só há versões de segurança deste jogo".into(),
        };
    };
    let meta = match load_meta(dest_root, game_id, &r.stem) {
        Ok(m) => m,
        Err(e) => return erro(e),
    };
    let plan = match plan_restore(
        dest_root,
        game_id,
        &r.stem,
        ctx,
        &vars_for(&meta, installed),
    ) {
        Ok(p) => p,
        Err(e) => return erro(e),
    };
    if plan.unresolved > 0 {
        return LoteResultado::PrecisaDePasta {
            faltando: plan.unresolved,
        };
    }
    match apply_restore(&plan, dest_root, ctx, &BTreeMap::new(), now) {
        Ok(rep) => LoteResultado::Restaurado {
            arquivos: rep.restored,
            seguranca: rep.safety_snapshot.map(|s| s.stem),
        },
        Err(e) => erro(e),
    }
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
            base: None,
            root: None,
            store_game_id: None,
            patterns: Vec::new(),
            steam_app_id: None,
            last_modified: None,
        };
        let BackupOutcome::Criado(r) = backup_game(&game, &a, &dest, 10, now()).unwrap() else {
            panic!()
        };
        (t, dest, r.stem)
    }

    /// Faz outra versão do save da Ana com `conteudo`, `horas` depois de `now()`.
    fn outra_versao_da_ana(root: &Path, dest: &Path, conteudo: &[u8], horas: i64) -> String {
        let a = ctx(root, "Ana", "111");
        let save = a.app_data.join("EldenRing/111");
        fs::write(save.join("ER0000.sl2"), conteudo).unwrap();
        let game = DetectedGame {
            id: "elden-ring".into(),
            name: "Elden Ring".into(),
            store: Store::Steam,
            save_roots: vec![save.clone()],
            files: vec![save.join("ER0000.sl2")],
            total_bytes: conteudo.len() as u64,
            cloud: CloudFlags::default(),
            source: Source::Manifesto,
            exe_hint: None,
            base: None,
            root: None,
            store_game_id: None,
            patterns: Vec::new(),
            steam_app_id: None,
            last_modified: None,
        };
        match backup_game(&game, &a, dest, 10, now() + Duration::hours(horas)).unwrap() {
            BackupOutcome::Criado(r) => r.stem,
            BackupOutcome::SemMudanca => panic!("sem mudança"),
        }
    }

    #[test]
    fn lote_restaura_a_mais_recente() {
        let (t, dest, _) = backup_da_ana();
        outra_versao_da_ana(t.path(), &dest, b"chefe final", 1);
        let b = ctx(t.path(), "Bruno", "222");
        let r = restore_latest(&dest, "elden-ring", &b, &[], now() + Duration::hours(2));
        assert_eq!(
            r,
            LoteResultado::Restaurado {
                arquivos: 1,
                seguranca: None
            }
        );
        let alvo = b.app_data.join("EldenRing/222/ER0000.sl2");
        assert_eq!(fs::read(alvo).unwrap(), b"chefe final");
    }

    #[test]
    fn lote_pula_versao_de_seguranca() {
        let (t, dest, _) = backup_da_ana();
        let nova = outra_versao_da_ana(t.path(), &dest, b"estado antigo do pc", 1);
        set_safety(&dest, "elden-ring", &nova).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        restore_latest(&dest, "elden-ring", &b, &[], now() + Duration::hours(2));
        let alvo = b.app_data.join("EldenRing/222/ER0000.sl2");
        assert_eq!(fs::read(alvo).unwrap(), b"progresso da ana");
    }

    #[test]
    fn lote_so_seguranca_vira_erro() {
        let (t, dest, stem) = backup_da_ana();
        set_safety(&dest, "elden-ring", &stem).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        match restore_latest(&dest, "elden-ring", &b, &[], now()) {
            LoteResultado::Erro { mensagem } => {
                assert!(
                    mensagem.contains("só há versões de segurança"),
                    "{mensagem}"
                )
            }
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn lote_sem_destino_resolvido_pede_pasta() {
        let (t, dest, stem) = backup_da_ana();
        let (_, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.files[0].template = "Q:/Jogos/Elden/ER0000.sl2".into();
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        assert_eq!(
            restore_latest(&dest, "elden-ring", &b, &[], now()),
            LoteResultado::PrecisaDePasta { faltando: 1 }
        );
    }

    #[cfg(windows)]
    #[test]
    fn lote_arquivo_travado_vira_erro() {
        use std::os::windows::fs::OpenOptionsExt;
        let (t, dest, _) = backup_da_ana();
        let b = ctx(t.path(), "Bruno", "222");
        let alvo = b.app_data.join("EldenRing/222/ER0000.sl2");
        fs::create_dir_all(alvo.parent().unwrap()).unwrap();
        fs::write(&alvo, b"progresso do bruno").unwrap();
        let _jogo = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&alvo)
            .unwrap();
        assert!(matches!(
            restore_latest(&dest, "elden-ring", &b, &[], now() + Duration::hours(1)),
            LoteResultado::Erro { .. }
        ));
    }

    #[test]
    fn restaurar_no_mesmo_pc_nao_marca_versao_normal_como_seguranca() {
        // O save do PC já é igual ao backup (ex.: a Steam Cloud devolveu antes).
        let (t, dest, stem) = backup_da_ana();
        let a = ctx(t.path(), "Ana", "111");
        let r = restore_latest(&dest, "elden-ring", &a, &[], now() + Duration::hours(1));
        assert_eq!(
            r,
            LoteResultado::Restaurado {
                arquivos: 1,
                seguranca: None
            }
        );
        let idx = LibraryIndex::load(&dest).unwrap();
        assert!(idx.games["elden-ring"].iter().all(|r| !r.seguranca));
        // De novo: continua restaurando a mesma versão (não vira "só há versões de segurança").
        let r2 = restore_latest(&dest, "elden-ring", &a, &[], now() + Duration::hours(2));
        assert!(matches!(r2, LoteResultado::Restaurado { .. }), "{r2:?}");
        assert_eq!(
            latest_restorable(&LibraryIndex::load(&dest).unwrap().games["elden-ring"])
                .unwrap()
                .stem,
            stem
        );
    }

    #[test]
    fn lote_jogo_inexistente_vira_erro() {
        let (t, dest, _) = backup_da_ana();
        let b = ctx(t.path(), "Bruno", "222");
        assert!(matches!(
            restore_latest(&dest, "nao-existe", &b, &[], now()),
            LoteResultado::Erro { .. }
        ));
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
    fn absoluto_sem_pasta_pai_fica_nao_resolvido() {
        let (t, dest, stem) = backup_da_ana();
        let (_, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        let fantasma = t
            .path()
            .join("SteamLibrary/steamapps/common/Jogo/ER0000.sl2");
        meta.files[0].template = fantasma.to_string_lossy().replace('\\', "/");
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        let plan = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        assert_eq!(
            plan.unresolved, 1,
            "pasta do jogo não existe neste PC: precisa perguntar"
        );
    }

    #[test]
    fn base_resolve_so_com_jogo_instalado() {
        let (t, dest, stem) = backup_da_ana();
        let (_, json) = snapshot_paths(&dest, "elden-ring", &stem);
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.files[0].template = "<base>/Saves/ER0000.sl2".into();
        fs::write(&json, serde_json::to_vec(&meta).unwrap()).unwrap();
        let b = ctx(t.path(), "Bruno", "222");
        let sem = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        assert_eq!(sem.unresolved, 1);
        let base = t.path().join("Jogos/Elden");
        let vars = GameVars {
            base: Some(base.clone()),
            ..Default::default()
        };
        let com = plan_restore(&dest, "elden-ring", &stem, &b, &vars).unwrap();
        assert_eq!(com.unresolved, 0);
        assert_eq!(
            com.items[0].target.as_deref(),
            Some(base.join("Saves/ER0000.sl2").as_path())
        );
    }

    #[test]
    fn vars_do_jogo_instalado_no_pc_novo() {
        use crate::stores::InstalledGame;
        let (_t, dest, stem) = backup_da_ana();
        let mut meta = load_meta(&dest, "elden-ring", &stem).unwrap();
        meta.store_game_id = Some("1245620".into());
        let inst = vec![
            InstalledGame {
                name: "Outro".into(),
                store: Store::Steam,
                install_dir: PathBuf::from("E:/Outro"),
                store_game_id: Some("999".into()),
                store_user_id: None,
                store_root: None,
            },
            InstalledGame {
                name: "ELDEN RING".into(),
                store: Store::Steam,
                install_dir: PathBuf::from("E:/Steam/steamapps/common/ELDEN RING"),
                store_game_id: Some("1245620".into()),
                store_user_id: None,
                store_root: Some(PathBuf::from("C:/Steam")),
            },
        ];
        let v = vars_for(&meta, &inst);
        assert_eq!(
            v.base,
            Some(PathBuf::from("E:/Steam/steamapps/common/ELDEN RING"))
        );
        assert_eq!(v.root, Some(PathBuf::from("C:/Steam")));
        meta.store_game_id = None;
        assert_eq!(vars_for(&meta, &inst), GameVars::default());
    }

    fn plano_com_dois_nao_resolvidos(t: &Path) -> RestorePlan {
        let item = |zip: &str| RestoreItem {
            zip_path: zip.into(),
            template: format!("Q:/{zip}"),
            target: None,
            exists: false,
            existing_modified: None,
        };
        let mut ok = item("pronto/c.sav");
        ok.target = Some(t.join("ja/c.sav"));
        RestorePlan {
            game_id: "x".into(),
            game_name: "X".into(),
            stem: "s".into(),
            items: vec![item("slot1/save.dat"), item("slot2/save.dat"), ok],
            unresolved: 2,
            conflicts: 0,
        }
    }

    #[test]
    fn overrides_validam_pasta_e_duplicados() {
        let t = tempfile::tempdir().unwrap();
        let plan = plano_com_dois_nao_resolvidos(t.path());
        let pasta = t.path().to_string_lossy().into_owned();
        // mesma pasta para os dois save.dat → iriam para o mesmo arquivo
        let dup = BTreeMap::from([
            ("slot1/save.dat".to_string(), pasta.clone()),
            ("slot2/save.dat".to_string(), pasta.clone()),
        ]);
        assert!(resolve_overrides(&plan, &dup).is_err());
        // pasta relativa ou vazia
        let rel = BTreeMap::from([("slot1/save.dat".to_string(), String::new())]);
        assert!(resolve_overrides(&plan, &rel).is_err());
        // item que já tem destino não aceita override
        let ja = BTreeMap::from([("pronto/c.sav".to_string(), pasta.clone())]);
        assert!(resolve_overrides(&plan, &ja).unwrap().is_empty());
        // válido
        let um = BTreeMap::from([("slot1/save.dat".to_string(), pasta.clone())]);
        let r = resolve_overrides(&plan, &um).unwrap();
        assert_eq!(r["slot1/save.dat"], t.path().join("save.dat"));
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
        assert!(r.seguranca, "marcada como versão de segurança");
        assert!(safety.seguranca);
        assert_eq!(fs::read(&alvo).unwrap(), b"progresso da ana");
        let meta = load_meta(&dest, "elden-ring", &safety.stem).unwrap();
        assert_eq!(meta.files.len(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn destino_em_uso_nao_restaura_nada() {
        use std::os::windows::fs::OpenOptionsExt;
        let (t, dest, stem) = backup_da_ana();
        let b = ctx(t.path(), "Bruno", "222");
        let alvo = b.app_data.join("EldenRing/222/ER0000.sl2");
        fs::create_dir_all(alvo.parent().unwrap()).unwrap();
        fs::write(&alvo, b"progresso do bruno").unwrap();
        // O jogo aberto deixa ler, mas não escrever (FILE_SHARE_READ = 1).
        let _jogo = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&alvo)
            .unwrap();
        let plan = plan_restore(&dest, "elden-ring", &stem, &b, &GameVars::default()).unwrap();
        let err = apply_restore(&plan, &dest, &b, &BTreeMap::new(), now()).unwrap_err();
        assert!(matches!(err, CoreError::ArquivoTravado(_)), "{err:?}");
        drop(_jogo);
        assert_eq!(fs::read(&alvo).unwrap(), b"progresso do bruno");
        let idx = LibraryIndex::load(&dest).unwrap();
        assert_eq!(
            idx.games["elden-ring"].len(),
            1,
            "não deve criar versão de segurança à toa"
        );
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
