//! Motor de backup: uma versão = um zip + um JSON legível, com índice atômico.
//!
//! ```text
//! <destino>\SaveGuard\
//!   biblioteca.json
//!   <jogo-id>\2026-11-20_21-40-05.zip
//!   <jogo-id>\2026-11-20_21-40-05.json
//! ```

mod library;
mod retention;
mod snapshot;

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub use chrono::{DateTime, Utc};
pub use library::{LibraryIndex, SnapshotRef, root as backup_root};
pub use snapshot::{FileEntry, SnapshotMeta};

use crate::CoreError;
use crate::paths::{GameVars, PathContext, contract_with, steam_id64};
use crate::scan::DetectedGame;
pub(crate) use library::write_atomic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupOutcome {
    Criado(SnapshotRef),
    SemMudanca,
}

/// Ponto de falha simulado nos testes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailPoint {
    Nenhum,
    #[cfg_attr(not(test), allow(dead_code))]
    AntesDoRename,
    /// Simula o jogo gravando o primeiro arquivo entre a leitura e a cópia.
    #[cfg_attr(not(test), allow(dead_code))]
    ModificarEntrePassadas,
}

/// Erros de compartilhamento do Windows (arquivo aberto por outro processo).
fn travado(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32) | Some(33))
}

fn open_original(path: &Path) -> Result<fs::File, CoreError> {
    fs::File::open(path).map_err(|e| {
        if travado(&e) {
            CoreError::ArquivoTravado(path.to_path_buf())
        } else {
            e.into()
        }
    })
}

/// Lê o arquivo em blocos (sem carregar tudo na memória), mandando cada bloco para
/// `sink`. Devolve (tamanho, BLAKE3).
fn stream_file(
    path: &Path,
    mut sink: impl FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<(u64, String), CoreError> {
    let mut f = open_original(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| {
            if travado(&e) {
                CoreError::ArquivoTravado(path.to_path_buf())
            } else {
                CoreError::Io(e)
            }
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        sink(&buf[..n])?;
        size += n as u64;
    }
    Ok((size, hasher.finalize().to_hex().to_string()))
}

/// Template com marcadores (`<base>`, `<root>`, pastas do usuário). A pasta com o ID
/// da conta da loja vira `<storeUserId>` (ou `<storeUserId64>` quando é o SteamID64).
pub(crate) fn template_for(path: &Path, ctx: &PathContext, game: &DetectedGame) -> String {
    let vars = GameVars {
        base: game.base.clone(),
        root: game.root.clone(),
        ..Default::default()
    };
    let t = contract_with(path, ctx, &vars);
    let Some(id3) = ctx
        .store_user_id
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "*")
    else {
        return t;
    };
    let id64 = steam_id64(id3);
    t.split('/')
        .map(|seg| {
            if seg == id3 {
                "<storeUserId>"
            } else if Some(seg) == id64.as_deref() {
                "<storeUserId64>"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn source_pc() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

fn fmt_err(e: impl std::fmt::Display) -> CoreError {
    CoreError::Formato(e.to_string())
}

/// Faz backup de um jogo para `dest_root`. Só lê os originais.
pub fn backup_game(
    game: &DetectedGame,
    ctx: &PathContext,
    dest_root: &Path,
    keep: usize,
    now: DateTime<Utc>,
) -> Result<BackupOutcome, CoreError> {
    backup_game_inner(game, ctx, dest_root, keep, now, FailPoint::Nenhum)
}

pub(crate) fn backup_game_inner(
    game: &DetectedGame,
    ctx: &PathContext,
    dest_root: &Path,
    keep: usize,
    now: DateTime<Utc>,
    fail: FailPoint,
) -> Result<BackupOutcome, CoreError> {
    if !dest_root.is_dir() {
        return Err(CoreError::DestinoIndisponivel(dest_root.to_path_buf()));
    }

    // 1. Primeira passada: lê cada arquivo em blocos só para calcular o hash
    // (arquivo travado = erro antes de gravar qualquer coisa; nada fica na memória).
    let mut items: Vec<(FileEntry, &PathBuf)> = Vec::new();
    for path in &game.files {
        let (size, hash) = stream_file(path, |_| Ok(()))?;
        let template = template_for(path, ctx, game);
        let entry = FileEntry {
            zip_path: snapshot::zip_path_for(&template),
            template,
            size,
            blake3: hash,
        };
        items.push((entry, path));
    }
    items.sort_by(|a, b| a.0.template.cmp(&b.0.template));
    let mut h = blake3::Hasher::new();
    for (e, _) in &items {
        h.update(e.template.as_bytes());
        h.update(e.blake3.as_bytes());
    }
    let content_hash = h.finalize().to_hex().to_string();

    let mut index = LibraryIndex::load(dest_root)?;
    let refs = index.games.entry(game.id.clone()).or_default();
    if refs
        .iter()
        .max_by_key(|r| r.created)
        .is_some_and(|r| r.content_hash == content_hash)
    {
        return Ok(BackupOutcome::SemMudanca);
    }

    // 2. Nome único para a versão.
    let dir = backup_root(dest_root).join(&game.id);
    fs::create_dir_all(&dir)?;
    let base = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let mut stem = base.clone();
    let mut n = 2;
    while dir.join(format!("{stem}.zip")).exists() || refs.iter().any(|r| r.stem == stem) {
        stem = format!("{base}_{n}");
        n += 1;
    }

    let total_bytes = items.iter().map(|(e, _)| e.size).sum();
    let meta = SnapshotMeta {
        game_id: game.id.clone(),
        game_name: game.name.clone(),
        created: now,
        source_pc: source_pc(),
        files: items.iter().map(|(e, _)| e.clone()).collect(),
        total_bytes,
        content_hash: content_hash.clone(),
        pinned: false,
        store: game.store,
        store_user_id: ctx.store_user_id.clone().filter(|s| s != "*"),
        store_game_id: game.store_game_id.clone(),
    };

    // 3. Grava .tmp e só renomeia quando tudo estiver no disco.
    let zip_final = dir.join(format!("{stem}.zip"));
    let zip_tmp = dir.join(format!("{stem}.zip.tmp"));
    let json_final = dir.join(format!("{stem}.json"));
    let write = || -> Result<(), CoreError> {
        let file = fs::File::create(&zip_tmp)?;
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        if fail == FailPoint::ModificarEntrePassadas {
            fs::OpenOptions::new()
                .append(true)
                .open(&game.files[0])?
                .write_all(b"gravando")?;
        }
        // 2ª passada: copia em blocos para o zip e confere que o arquivo não mudou
        // desde a primeira (jogo gravando sem travar o arquivo = versão pela metade).
        for (e, path) in &items {
            let opts = opts.large_file(e.size >= u32::MAX as u64);
            zw.start_file(e.zip_path.as_str(), opts).map_err(fmt_err)?;
            let (size, hash) = stream_file(path, |chunk| zw.write_all(chunk))?;
            if size != e.size || hash != e.blake3 {
                return Err(CoreError::ArquivoMudou((*path).clone()));
            }
        }
        let file = zw.finish().map_err(fmt_err)?;
        file.sync_all()?;
        drop(file);
        if fail == FailPoint::AntesDoRename {
            return Err(CoreError::Io(std::io::Error::other("falha simulada")));
        }
        fs::rename(&zip_tmp, &zip_final)?;
        write_atomic(
            &json_final,
            &serde_json::to_vec_pretty(&meta).map_err(fmt_err)?,
        )
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&zip_tmp);
        let _ = fs::remove_file(&zip_final);
        let _ = fs::remove_file(&json_final);
        return Err(e);
    }

    // 4. Índice (atômico).
    let r = SnapshotRef {
        stem,
        created: now,
        pinned: false,
        content_hash,
        total_bytes,
    };
    refs.push(r.clone());
    refs.sort_by_key(|r| r.created);
    index.save_atomic(dest_root)?;

    // 5. Retenção, só depois do índice gravado.
    apply_retention(dest_root, &game.id, keep, &mut index)?;
    Ok(BackupOutcome::Criado(r))
}

fn apply_retention(
    dest_root: &Path,
    game_id: &str,
    keep: usize,
    index: &mut LibraryIndex,
) -> Result<(), CoreError> {
    let Some(refs) = index.games.get_mut(game_id) else {
        return Ok(());
    };
    let delete = retention::to_delete(refs, keep);
    if delete.is_empty() {
        return Ok(());
    }
    refs.retain(|r| !delete.contains(&r.stem));
    index.save_atomic(dest_root)?;
    let dir = backup_root(dest_root).join(game_id);
    for stem in delete {
        let _ = fs::remove_file(dir.join(format!("{stem}.zip")));
        let _ = fs::remove_file(dir.join(format!("{stem}.json")));
    }
    Ok(())
}

/// Caminhos do zip e do json de uma versão.
pub fn snapshot_paths(dest_root: &Path, game_id: &str, stem: &str) -> (PathBuf, PathBuf) {
    let dir = backup_root(dest_root).join(game_id);
    (
        dir.join(format!("{stem}.zip")),
        dir.join(format!("{stem}.json")),
    )
}

/// Lê os metadados de uma versão.
pub fn load_meta(dest_root: &Path, game_id: &str, stem: &str) -> Result<SnapshotMeta, CoreError> {
    let (_, json) = snapshot_paths(dest_root, game_id, stem);
    serde_json::from_str(&fs::read_to_string(json)?).map_err(fmt_err)
}

/// Fixa (ou solta) uma versão: fixadas nunca são apagadas pela retenção.
pub fn set_pinned(
    dest_root: &Path,
    game_id: &str,
    stem: &str,
    pinned: bool,
) -> Result<(), CoreError> {
    let mut index = LibraryIndex::load(dest_root)?;
    let r = index
        .games
        .get_mut(game_id)
        .and_then(|v| v.iter_mut().find(|r| r.stem == stem))
        .ok_or_else(|| CoreError::Formato(format!("versão {game_id}/{stem} não existe")))?;
    r.pinned = pinned;
    index.save_atomic(dest_root)?;
    let mut meta = load_meta(dest_root, game_id, stem)?;
    meta.pinned = pinned;
    let (_, json) = snapshot_paths(dest_root, game_id, stem);
    write_atomic(&json, &serde_json::to_vec_pretty(&meta).map_err(fmt_err)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{CloudFlags, Store};
    use crate::paths::PathContext;
    use crate::protection::BackupHistory;
    use crate::scan::{DetectedGame, Source};
    use chrono::{Duration, TimeZone, Utc};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn ctx(root: &Path) -> PathContext {
        let home = root.join("Users/Ana");
        PathContext {
            app_data: home.join("AppData/Roaming"),
            local_app_data: home.join("AppData/Local"),
            local_app_data_low: home.join("AppData/LocalLow"),
            documents: home.join("Documents"),
            public: root.join("Users/Public"),
            program_data: root.join("ProgramData"),
            os_user_name: "Ana".into(),
            store_user_id: Some("111".into()),
            home,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 21, 40, 5).unwrap()
    }

    struct Fixture {
        _t: tempfile::TempDir,
        ctx: PathContext,
        dest: PathBuf,
        game: DetectedGame,
    }

    fn fixture() -> Fixture {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        let save = c.app_data.join("EldenRing/111");
        fs::create_dir_all(&save).unwrap();
        fs::write(save.join("ER0000.sl2"), b"slot").unwrap();
        fs::write(save.join("ER0000.sl2.bak"), b"slot-bak").unwrap();
        let dest = t.path().join("Destino");
        fs::create_dir_all(&dest).unwrap();
        let game = DetectedGame {
            id: "elden-ring".into(),
            name: "Elden Ring".into(),
            store: Store::Steam,
            save_roots: vec![save.clone()],
            files: vec![save.join("ER0000.sl2"), save.join("ER0000.sl2.bak")],
            total_bytes: 12,
            cloud: CloudFlags::default(),
            source: Source::Manifesto,
            exe_hint: None,
            base: None,
            root: None,
            store_game_id: None,
        };
        Fixture {
            _t: t,
            ctx: c,
            dest,
            game,
        }
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let mut z = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        (0..z.len())
            .map(|i| z.by_index(i).unwrap().name().unwrap().to_string())
            .collect()
    }

    #[test]
    fn cria_versao_e_indice() {
        let f = fixture();
        let out = backup_game(&f.game, &f.ctx, &f.dest, 10, now()).unwrap();
        let BackupOutcome::Criado(r) = out else {
            panic!("esperava Criado")
        };
        assert_eq!(r.stem, "2026-11-20_21-40-05");
        let dir = f.dest.join("SaveGuard/elden-ring");
        assert!(dir.join("2026-11-20_21-40-05.zip").is_file());
        assert!(f.dest.join("SaveGuard/biblioteca.json").is_file());
        assert_eq!(zip_names(&dir.join("2026-11-20_21-40-05.zip")).len(), 2);
        let meta: SnapshotMeta = serde_json::from_str(
            &fs::read_to_string(dir.join("2026-11-20_21-40-05.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta.files.len(), 2);
        assert!(
            meta.files.iter().all(|e| e.template.starts_with('<')),
            "{:?}",
            meta.files
        );
        assert_eq!(
            meta.files[0].template,
            "<winAppData>/EldenRing/<storeUserId>/ER0000.sl2"
        );
        assert_eq!(meta.store_user_id.as_deref(), Some("111"));
        let idx = LibraryIndex::load(&f.dest).unwrap();
        assert_eq!(idx.games["elden-ring"].len(), 1);
        assert_eq!(idx.last_backup("elden-ring"), Some(now()));
    }

    #[test]
    fn template_usa_base_e_id64() {
        let f = fixture();
        let mut g = f.game.clone();
        let base = f.dest.parent().unwrap().join("Jogos/MeuJogo");
        fs::create_dir_all(base.join("Saves")).unwrap();
        fs::write(base.join("Saves/a.sav"), b"x").unwrap();
        // id3 "111" do contexto = id64 76561197960265839
        let er = f.ctx.app_data.join("EldenRing/76561197960265839");
        fs::create_dir_all(&er).unwrap();
        fs::write(er.join("ER0000.sl2"), b"y").unwrap();
        g.base = Some(base.clone());
        g.store_game_id = Some("123".into());
        g.files = vec![base.join("Saves/a.sav"), er.join("ER0000.sl2")];
        let BackupOutcome::Criado(r) = backup_game(&g, &f.ctx, &f.dest, 10, now()).unwrap() else {
            panic!()
        };
        let meta = load_meta(&f.dest, "elden-ring", &r.stem).unwrap();
        let t: Vec<_> = meta.files.iter().map(|e| e.template.as_str()).collect();
        assert!(t.contains(&"<base>/Saves/a.sav"), "{t:?}");
        assert!(
            t.contains(&"<winAppData>/EldenRing/<storeUserId64>/ER0000.sl2"),
            "{t:?}"
        );
        assert_eq!(meta.store_game_id.as_deref(), Some("123"));
    }

    #[test]
    fn sem_mudanca_nao_cria() {
        let f = fixture();
        backup_game(&f.game, &f.ctx, &f.dest, 10, now()).unwrap();
        let out = backup_game(&f.game, &f.ctx, &f.dest, 10, now() + Duration::seconds(60)).unwrap();
        assert_eq!(out, BackupOutcome::SemMudanca);
        assert_eq!(
            LibraryIndex::load(&f.dest).unwrap().games["elden-ring"].len(),
            1
        );
    }

    #[test]
    fn mesmo_segundo_nao_sobrescreve() {
        let f = fixture();
        backup_game(&f.game, &f.ctx, &f.dest, 10, now()).unwrap();
        fs::write(&f.game.files[0], b"mudou").unwrap();
        let BackupOutcome::Criado(r) = backup_game(&f.game, &f.ctx, &f.dest, 10, now()).unwrap()
        else {
            panic!()
        };
        assert_ne!(r.stem, "2026-11-20_21-40-05");
        assert_eq!(
            LibraryIndex::load(&f.dest).unwrap().games["elden-ring"].len(),
            2
        );
    }

    #[test]
    fn retencao_mantem_10_e_fixadas() {
        let f = fixture();
        for i in 0..12 {
            fs::write(&f.game.files[0], format!("versao {i}")).unwrap();
            backup_game(&f.game, &f.ctx, &f.dest, 10, now() + Duration::seconds(i)).unwrap();
            if i == 0 {
                set_pinned(&f.dest, "elden-ring", "2026-11-20_21-40-05", true).unwrap();
            }
        }
        let refs = &LibraryIndex::load(&f.dest).unwrap().games["elden-ring"];
        assert_eq!(refs.len(), 11);
        assert!(
            refs.iter()
                .any(|r| r.stem == "2026-11-20_21-40-05" && r.pinned)
        );
        assert!(
            !refs.iter().any(|r| r.stem == "2026-11-20_21-40-06"),
            "a 2ª deveria sair"
        );
        let zips = fs::read_dir(f.dest.join("SaveGuard/elden-ring"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "zip"))
            .count();
        assert_eq!(zips, 11);
    }

    #[cfg(windows)]
    #[test]
    fn arquivo_travado_nao_grava() {
        use std::os::windows::fs::OpenOptionsExt;
        let f = fixture();
        let _lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&f.game.files[0])
            .unwrap();
        let err = backup_game(&f.game, &f.ctx, &f.dest, 10, now()).unwrap_err();
        assert!(matches!(err, CoreError::ArquivoTravado(_)), "{err:?}");
        assert!(
            !f.dest.join("SaveGuard/elden-ring").exists()
                || fs::read_dir(f.dest.join("SaveGuard/elden-ring"))
                    .unwrap()
                    .next()
                    .is_none()
        );
        assert!(!f.dest.join("SaveGuard/biblioteca.json").exists());
    }

    #[test]
    fn indice_corrompido_nao_some() {
        let f = fixture();
        backup_game(&f.game, &f.ctx, &f.dest, 10, now()).unwrap();
        let before = fs::read_to_string(f.dest.join("SaveGuard/biblioteca.json")).unwrap();
        fs::write(&f.game.files[0], b"mudou").unwrap();
        let err = backup_game_inner(
            &f.game,
            &f.ctx,
            &f.dest,
            10,
            now() + Duration::seconds(1),
            FailPoint::AntesDoRename,
        );
        assert!(err.is_err());
        let after = fs::read_to_string(f.dest.join("SaveGuard/biblioteca.json")).unwrap();
        assert_eq!(before, after);
        serde_json::from_str::<serde_json::Value>(&after).unwrap();
        let leftovers = fs::read_dir(f.dest.join("SaveGuard/elden-ring"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0, "temporários devem ser limpos");
    }

    #[test]
    fn arquivo_que_muda_durante_a_copia_nao_vira_versao() {
        let f = fixture();
        let err = backup_game_inner(
            &f.game,
            &f.ctx,
            &f.dest,
            10,
            now(),
            FailPoint::ModificarEntrePassadas,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::ArquivoMudou(_)), "{err:?}");
        let idx = LibraryIndex::load(&f.dest).unwrap();
        assert!(idx.games.get("elden-ring").is_none_or(|v| v.is_empty()));
        let dir = f.dest.join("SaveGuard/elden-ring");
        let sobras = fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
        assert_eq!(sobras, 0, "nada pode sobrar no destino");
    }

    #[test]
    fn arquivo_grande_em_streaming() {
        let f = fixture();
        let big = vec![7u8; 20 * 1024 * 1024];
        fs::write(&f.game.files[0], &big).unwrap();
        let BackupOutcome::Criado(r) = backup_game(&f.game, &f.ctx, &f.dest, 10, now()).unwrap()
        else {
            panic!()
        };
        let mut z = zip::ZipArchive::new(
            fs::File::open(f.dest.join(format!("SaveGuard/elden-ring/{}.zip", r.stem))).unwrap(),
        )
        .unwrap();
        let name = z.by_index(0).unwrap().name().unwrap().to_string();
        let size = (0..z.len())
            .map(|i| z.by_index(i).unwrap().size())
            .max()
            .unwrap();
        assert_eq!(size, big.len() as u64, "{name}");
    }

    #[test]
    fn destino_inexistente() {
        let f = fixture();
        let err = backup_game(&f.game, &f.ctx, Path::new("Z:/nao-existe"), 10, now()).unwrap_err();
        assert!(matches!(err, CoreError::DestinoIndisponivel(_)), "{err:?}");
    }
}
