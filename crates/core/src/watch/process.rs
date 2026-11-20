//! Qual processo em execução pertence a qual jogo.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::WatchEvent;
use crate::paths::norm;
use crate::scan::DetectedGame;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub exe: PathBuf,
}

/// Como reconhecer o processo de um jogo.
struct Rule {
    id: String,
    /// Executável exato (emuladores, jogos manuais), em minúsculas com `/`.
    exe: Option<String>,
    /// Pasta de instalação: qualquer executável dentro dela é do jogo.
    base: Option<String>,
}

impl Rule {
    fn matches(&self, exe: &str) -> bool {
        self.exe.as_deref() == Some(exe)
            || self
                .base
                .as_deref()
                .is_some_and(|b| exe.starts_with(&format!("{b}/")))
    }
}

/// Compara a lista de processos de agora com a anterior e gera abriu/fechou.
pub struct ProcessTracker {
    rules: Vec<Rule>,
    running: BTreeSet<String>,
}

impl ProcessTracker {
    pub fn new(games: &[DetectedGame]) -> Self {
        let key = |p: &PathBuf| norm(p).to_lowercase();
        let rules = games
            .iter()
            .filter(|g| g.base.is_some() || g.exe_hint.is_some())
            .map(|g| Rule {
                id: g.id.clone(),
                exe: g.exe_hint.as_ref().map(key),
                base: g.base.as_ref().map(key),
            })
            .collect();
        ProcessTracker {
            rules,
            running: BTreeSet::new(),
        }
    }

    /// Tracker novo (varredura refeita) que lembra quais jogos estavam abertos:
    /// se um deles fechou no meio tempo, a próxima `update` gera o `JogoFechou`.
    pub fn with_running(games: &[DetectedGame], running: BTreeSet<String>) -> Self {
        let mut t = Self::new(games);
        t.running = running;
        t
    }

    pub fn running(&self) -> BTreeSet<String> {
        self.running.clone()
    }

    pub fn update(&mut self, procs: &[ProcInfo]) -> Vec<WatchEvent> {
        let exes: Vec<String> = procs.iter().map(|p| norm(&p.exe).to_lowercase()).collect();
        let now: BTreeSet<String> = self
            .rules
            .iter()
            .filter(|r| exes.iter().any(|e| r.matches(e)))
            .map(|r| r.id.clone())
            .collect();
        let mut events: Vec<WatchEvent> = now
            .difference(&self.running)
            .map(|id| WatchEvent::JogoAbriu(id.clone()))
            .collect();
        events.extend(
            self.running
                .difference(&now)
                .map(|id| WatchEvent::JogoFechou(id.clone())),
        );
        self.running = now;
        events
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::manifest::{CloudFlags, Store};
    use crate::scan::{DetectedGame, Source};
    use std::path::PathBuf;

    pub(crate) fn game(
        id: &str,
        base: Option<&str>,
        exe: Option<&str>,
        root: &str,
    ) -> DetectedGame {
        DetectedGame {
            id: id.into(),
            name: id.into(),
            store: Store::SemLoja,
            save_roots: vec![PathBuf::from(root)],
            files: vec![],
            total_bytes: 0,
            cloud: CloudFlags::default(),
            source: Source::Manifesto,
            exe_hint: exe.map(PathBuf::from),
            base: base.map(PathBuf::from),
            root: None,
            store_game_id: None,
            patterns: Vec::new(),
        }
    }

    fn p(pid: u32, exe: &str) -> ProcInfo {
        ProcInfo {
            pid,
            exe: PathBuf::from(exe),
        }
    }

    #[test]
    fn exe_dentro_da_pasta_do_jogo() {
        let mut t = ProcessTracker::new(&[game("hades", Some("D:/Jogos/Hades"), None, "C:/s")]);
        let procs = [p(1, r"D:\jogos\hades\x64\Hades.exe")];
        assert_eq!(
            t.update(&procs),
            vec![WatchEvent::JogoAbriu("hades".into())]
        );
        assert!(t.update(&procs).is_empty());
        assert_eq!(t.update(&[]), vec![WatchEvent::JogoFechou("hades".into())]);
    }

    #[test]
    fn exe_hint_de_emulador() {
        let mut t =
            ProcessTracker::new(&[game("pcsx2", None, Some("E:/Emu/pcsx2-qt.exe"), "C:/s")]);
        assert!(t.update(&[p(1, "E:/Emu/outro.exe")]).is_empty());
        assert_eq!(
            t.update(&[p(2, "e:/emu/PCSX2-QT.exe")]),
            vec![WatchEvent::JogoAbriu("pcsx2".into())]
        );
    }

    #[test]
    fn prefixo_parecido_nao_casa() {
        let mut t = ProcessTracker::new(&[game("hades", Some("D:/Jogos/Hades"), None, "C:/s")]);
        assert!(t.update(&[p(1, "D:/Jogos/Hades2/h.exe")]).is_empty());
    }

    #[test]
    fn dois_processos_do_mesmo_jogo_um_evento() {
        let mut t = ProcessTracker::new(&[game("g", Some("D:/G"), None, "C:/s")]);
        let ambos = [p(1, "D:/G/launcher.exe"), p(2, "D:/G/bin/game.exe")];
        assert_eq!(t.update(&ambos), vec![WatchEvent::JogoAbriu("g".into())]);
        assert!(t.update(&[p(2, "D:/G/bin/game.exe")]).is_empty());
        assert_eq!(t.update(&[]), vec![WatchEvent::JogoFechou("g".into())]);
    }

    #[test]
    fn recriar_mantem_jogos_abertos() {
        // Nova varredura com o jogo aberto: o fechamento depois tem que ser notado.
        let g = game("hades", Some("D:/Jogos/Hades"), None, "C:/s");
        let mut t = ProcessTracker::new(std::slice::from_ref(&g));
        t.update(&[p(1, "D:/Jogos/Hades/h.exe")]);
        let mut t2 = ProcessTracker::with_running(&[g], t.running());
        assert!(t2.update(&[p(1, "D:/Jogos/Hades/h.exe")]).is_empty());
        assert_eq!(t2.update(&[]), vec![WatchEvent::JogoFechou("hades".into())]);
    }

    #[test]
    fn jogo_sem_base_nao_rastreia() {
        let mut t = ProcessTracker::new(&[game("x", None, None, "C:/s")]);
        assert!(t.update(&[p(1, "C:/s/x.exe")]).is_empty());
        assert!(t.update(&[]).is_empty());
    }

    #[test]
    fn eventos_em_ordem_de_id() {
        let mut t = ProcessTracker::new(&[
            game("b", Some("D:/B"), None, "C:/b"),
            game("a", Some("D:/A"), None, "C:/a"),
        ]);
        let ev = t.update(&[p(1, "D:/B/b.exe"), p(2, "D:/A/a.exe")]);
        assert_eq!(
            ev,
            vec![
                WatchEvent::JogoAbriu("a".into()),
                WatchEvent::JogoAbriu("b".into())
            ]
        );
    }
}
