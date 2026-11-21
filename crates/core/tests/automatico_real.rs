//! Ciclo automático com um processo real (não mexe nos saves do usuário):
//! um "jogo" falso (cópia do ping.exe numa pasta temporária) abre, grava o save,
//! fecha, e o backup sai 10 s depois, como no app.
//!
//! `cargo test -p savesgg-core --test automatico_real -- --ignored --nocapture`

use std::fs;
use std::time::{Duration, Instant};

use chrono::Utc;
use savesgg_core::backup::{BackupOutcome, LibraryIndex, backup_game};
use savesgg_core::manifest::{CloudFlags, Store};
use savesgg_core::paths::PathContext;
use savesgg_core::scan::{DetectedGame, Source};
use savesgg_core::watch::{ProcInfo, ProcessTracker, Scheduler, WatchEvent};

fn processos(sys: &mut sysinfo::System) -> Vec<ProcInfo> {
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

#[cfg(windows)]
#[test]
#[ignore]
fn ciclo_automatico_com_processo_real() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().join("JogoFalso");
    fs::create_dir_all(&base).unwrap();
    let exe = base.join("jogo-falso.exe");
    fs::copy(r"C:\Windows\System32\PING.EXE", &exe).unwrap();
    let saves = t.path().join("Saves");
    fs::create_dir_all(&saves).unwrap();
    fs::write(saves.join("slot.sav"), b"antes de jogar").unwrap();
    let dest = t.path().join("Destino");
    fs::create_dir_all(&dest).unwrap();

    let game = DetectedGame {
        id: "jogo-falso".into(),
        name: "Jogo Falso".into(),
        store: Store::SemLoja,
        save_roots: vec![saves.clone()],
        files: vec![saves.join("slot.sav")],
        total_bytes: 0,
        cloud: CloudFlags::default(),
        source: Source::Manual,
        exe_hint: None,
        base: Some(base.clone()),
        root: None,
        store_game_id: None,
        patterns: Vec::new(),
        steam_app_id: None,
        last_modified: None,
    };
    let ctx = PathContext::from_system();
    let mut sys = sysinfo::System::new().expect("sysinfo");
    let mut tracker = ProcessTracker::new(std::slice::from_ref(&game));
    let mut sched = Scheduler::new();
    let start = Instant::now();

    // "Joga" por ~4 s e grava o save no meio.
    let mut child = std::process::Command::new(&exe)
        .args(["-n", "5", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut eventos = Vec::new();
    let mut backup = None;
    while start.elapsed() < Duration::from_secs(40) && backup.is_none() {
        let now = start.elapsed().as_secs();
        for ev in tracker.update(&processos(&mut sys)) {
            eprintln!("{now:>3}s {ev:?}");
            eventos.push(ev.clone());
            if matches!(ev, WatchEvent::JogoAbriu(_)) {
                fs::write(saves.join("slot.sav"), b"progresso novo").unwrap();
            }
            sched.on_event(ev, now);
        }
        for id in sched.due(now) {
            assert_eq!(id, "jogo-falso");
            eprintln!("{now:>3}s backup vencido");
            backup = Some((
                now,
                backup_game(&game, &ctx, &dest, 10, Utc::now()).unwrap(),
            ));
        }
        if let Ok(Some(_)) = child.try_wait() {}
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        matches!(eventos.first(), Some(WatchEvent::JogoAbriu(_))),
        "{eventos:?}"
    );
    assert!(
        eventos
            .iter()
            .any(|e| matches!(e, WatchEvent::JogoFechou(_))),
        "{eventos:?}"
    );
    let (quando, out) = backup.expect("o backup automático não aconteceu");
    assert!(matches!(out, BackupOutcome::Criado(_)));
    assert!(quando >= 10, "saiu cedo demais: {quando}s");
    let idx = LibraryIndex::load(&dest).unwrap();
    assert_eq!(idx.games["jogo-falso"].len(), 1);
}
