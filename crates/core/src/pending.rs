//! Fila de pendentes: backups automáticos feitos enquanto o destino estava fora
//! (pendrive desplugado, pasta da nuvem sumida) e enviados quando ele volta.
//!
//! Cada destino tem uma pasta própria em `%LOCALAPPDATA%\SaveGuard\pendentes\<hash>`,
//! com o mesmo formato de um destino normal (`SaveGuard\<jogo>\<versão>.zip/.json`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::CoreError;
use crate::backup::{LibraryIndex, backup_root};

/// Pasta de pendentes de um destino (o id vira hash: tem `:` e `/`).
pub fn pending_root(base: &Path, destination_id: &str) -> PathBuf {
    base.join(&blake3::hash(destination_id.as_bytes()).to_hex()[..16])
}

/// Versões completas (`.zip` com `.json`) de uma pasta `SaveGuard`.
pub(crate) fn versions(root: &Path) -> Vec<(String, PathBuf, PathBuf)> {
    let Ok(games) = fs::read_dir(backup_root(root)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for g in games.flatten().filter(|e| e.path().is_dir()) {
        let game = g.file_name().to_string_lossy().into_owned();
        let Ok(files) = fs::read_dir(g.path()) else {
            continue;
        };
        for f in files.flatten() {
            let zip = f.path();
            if zip.extension().is_some_and(|x| x == "zip") {
                let json = zip.with_extension("json");
                if json.is_file() {
                    out.push((game.clone(), zip, json));
                }
            }
        }
    }
    out.sort();
    out
}

/// Quantas versões esperam envio.
pub fn pending_count(pending_root: &Path) -> usize {
    versions(pending_root).len()
}

fn copy_atomic(from: &Path, to: &Path) -> Result<(), CoreError> {
    let mut tmp = to.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::copy(from, &tmp)?;
    fs::OpenOptions::new().write(true).open(&tmp)?.sync_all()?;
    fs::rename(&tmp, to).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

/// Envia os pendentes para o destino. Copia o zip e depois o json (o índice do
/// destino se reconstrói pelos json), grava o índice e só então limpa o pendente.
pub fn flush_pending(pending_root: &Path, dest_root: &Path) -> Result<usize, CoreError> {
    flush_listed(pending_root, dest_root, &versions(pending_root))
}

/// Envia só as versões listadas e apaga do pendente só elas: uma versão que
/// entrar na fila durante o envio fica para a próxima vez.
pub(crate) fn flush_listed(
    pending_root: &Path,
    dest_root: &Path,
    list: &[(String, PathBuf, PathBuf)],
) -> Result<usize, CoreError> {
    if list.is_empty() {
        return Ok(0);
    }
    if !dest_root.is_dir() {
        return Err(CoreError::DestinoIndisponivel(dest_root.to_path_buf()));
    }
    let mut sent = 0;
    for (game, zip, json) in list {
        let dir = backup_root(dest_root).join(game);
        fs::create_dir_all(&dir)?;
        let (Some(zname), Some(jname)) = (zip.file_name(), json.file_name()) else {
            continue;
        };
        if dir.join(zname).is_file() && dir.join(jname).is_file() {
            continue;
        }
        copy_atomic(zip, &dir.join(zname))?;
        copy_atomic(json, &dir.join(jname))?;
        sent += 1;
    }
    LibraryIndex::load(dest_root)?.save_atomic(dest_root)?;
    // Só o que foi enviado sai da fila; o índice da fila é refeito pelo que sobrou.
    for (_, zip, json) in list {
        let _ = fs::remove_file(zip);
        let _ = fs::remove_file(json);
    }
    let rest = LibraryIndex::rebuild_from_sidecars(pending_root);
    if rest.games.values().all(|v| v.is_empty()) {
        let _ = fs::remove_dir_all(backup_root(pending_root));
    } else {
        rest.save_atomic(pending_root)?;
    }
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{BackupOutcome, LibraryIndex, backup_game};
    use crate::manifest::{CloudFlags, Store};
    use crate::paths::PathContext;
    use crate::scan::{DetectedGame, Source};
    use chrono::{TimeZone, Utc};
    use std::fs;

    struct Fx {
        t: tempfile::TempDir,
        ctx: PathContext,
        game: DetectedGame,
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
            patterns: Vec::new(),
        };
        Fx { t, ctx, game }
    }

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 22, 0, 0).unwrap()
    }

    #[test]
    fn pendente_vai_para_o_destino_quando_volta() {
        let f = fx();
        let p = pending_root(&f.t.path().join("pendentes"), "disco:abcd1234");
        fs::create_dir_all(&p).unwrap();
        let BackupOutcome::Criado(r) = backup_game(&f.game, &f.ctx, &p, 10, now()).unwrap() else {
            panic!()
        };
        assert_eq!(pending_count(&p), 1);
        let dest = f.t.path().join("Pendrive");
        fs::create_dir_all(&dest).unwrap();
        assert_eq!(flush_pending(&p, &dest).unwrap(), 1);
        let idx = LibraryIndex::load(&dest).unwrap();
        assert!(idx.games["jogo"].iter().any(|x| x.stem == r.stem));
        assert!(
            dest.join(format!("SaveGuard/jogo/{}.zip", r.stem))
                .is_file()
        );
        assert_eq!(pending_count(&p), 0);
    }

    #[test]
    fn nao_duplica_o_que_o_destino_ja_tem() {
        let f = fx();
        let p = pending_root(&f.t.path().join("pendentes"), "pasta:x");
        let dest = f.t.path().join("Destino");
        fs::create_dir_all(&p).unwrap();
        fs::create_dir_all(&dest).unwrap();
        backup_game(&f.game, &f.ctx, &p, 10, now()).unwrap();
        // a mesma versão já chegou ao destino por outro caminho
        let src = p.join("SaveGuard/jogo");
        fs::create_dir_all(dest.join("SaveGuard/jogo")).unwrap();
        for e in fs::read_dir(&src).unwrap().flatten() {
            fs::copy(e.path(), dest.join("SaveGuard/jogo").join(e.file_name())).unwrap();
        }
        assert_eq!(flush_pending(&p, &dest).unwrap(), 0);
        assert_eq!(pending_count(&p), 0);
    }

    #[test]
    fn envio_nao_apaga_versao_que_chegou_depois() {
        let f = fx();
        let p = pending_root(&f.t.path().join("pendentes"), "disco:9");
        let dest = f.t.path().join("Pendrive");
        fs::create_dir_all(&p).unwrap();
        fs::create_dir_all(&dest).unwrap();
        backup_game(&f.game, &f.ctx, &p, 10, now()).unwrap();
        let lista = versions(&p); // o envio já listou o que vai mandar...
        fs::write(&f.game.files[0], b"jogou mais").unwrap();
        // ...e uma versão nova entra na fila enquanto isso
        backup_game(
            &f.game,
            &f.ctx,
            &p,
            10,
            now() + chrono::Duration::seconds(60),
        )
        .unwrap();
        assert_eq!(flush_listed(&p, &dest, &lista).unwrap(), 1);
        assert_eq!(
            pending_count(&p),
            1,
            "a versão nova tem que continuar na fila"
        );
        // e a fila não pode achar que a versão nova já foi feita (índice sem restos)
        let idx = LibraryIndex::load(&p).unwrap();
        assert_eq!(idx.games["jogo"].len(), 1);
    }

    #[test]
    fn destino_indisponivel_mantem_pendente() {
        let f = fx();
        let p = pending_root(&f.t.path().join("pendentes"), "disco:1");
        fs::create_dir_all(&p).unwrap();
        backup_game(&f.game, &f.ctx, &p, 10, now()).unwrap();
        let err = flush_pending(&p, Path::new("Z:/nada")).unwrap_err();
        assert!(matches!(err, CoreError::DestinoIndisponivel(_)), "{err:?}");
        assert_eq!(pending_count(&p), 1);
    }

    #[test]
    fn ids_diferentes_pastas_diferentes() {
        let b = Path::new("C:/pendentes");
        let a = pending_root(b, "disco:1");
        let c = pending_root(b, "disco:2");
        assert_ne!(a, c);
        assert!(a.starts_with(b) && c.starts_with(b));
        assert_eq!(a, pending_root(b, "disco:1"));
    }

    #[test]
    fn sem_pendentes_zero() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(pending_count(&t.path().join("nada")), 0);
    }
}
