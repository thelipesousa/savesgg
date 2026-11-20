//! Decide quando fazer o backup automático de cada jogo.
//!
//! O tempo é sempre em segundos monotônicos (`Instant`), nunca a hora do relógio:
//! suspender o PC ou ajustar o relógio não pode adiantar nem atrasar backups.

use std::collections::{BTreeMap, BTreeSet};

/// Espera depois de o jogo fechar (o jogo pode gravar ao sair).
pub const ESPERA_APOS_FECHAR: u64 = 10;
/// Silêncio da pasta de save antes de copiar (jogos sem processo conhecido).
pub const SILENCIO_DA_PASTA: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    JogoAbriu(String),
    JogoFechou(String),
    PastaMudou(String),
}

#[derive(Debug, Default)]
pub struct Scheduler {
    /// jogo → quando o backup vence
    deadlines: BTreeMap<String, u64>,
    running: BTreeSet<String>,
    /// Pasta mudou com o jogo aberto: copia ao fechar.
    dirty: BTreeSet<String>,
    last_now: u64,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    fn clock(&mut self, now: u64) -> u64 {
        self.last_now = self.last_now.max(now);
        self.last_now
    }

    pub fn on_event(&mut self, ev: WatchEvent, now: u64) {
        let now = self.clock(now);
        match ev {
            WatchEvent::JogoAbriu(id) => {
                self.deadlines.remove(&id);
                self.running.insert(id);
            }
            WatchEvent::JogoFechou(id) => {
                self.running.remove(&id);
                self.dirty.remove(&id);
                self.deadlines.insert(id, now + ESPERA_APOS_FECHAR);
            }
            WatchEvent::PastaMudou(id) => {
                if self.running.contains(&id) {
                    self.dirty.insert(id);
                } else {
                    self.deadlines.insert(id, now + SILENCIO_DA_PASTA);
                }
            }
        }
    }

    /// Jogos cujo backup venceu (em ordem de id); já saem da agenda.
    pub fn due(&mut self, now: u64) -> Vec<String> {
        let now = self.clock(now);
        let ready: Vec<String> = self
            .deadlines
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ready {
            self.deadlines.remove(id);
        }
        ready
    }

    /// Jogos abertos agora.
    pub fn running(&self) -> Vec<String> {
        self.running.iter().cloned().collect()
    }

    /// Backups agendados que ainda não venceram.
    pub fn pending_count(&self) -> usize {
        self.deadlines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn fechar_agenda_em_10s() {
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::JogoFechou("a".into()), 100);
        assert!(sc.due(109).is_empty());
        assert_eq!(sc.due(110), s(&["a"]));
        assert!(sc.due(111).is_empty(), "não repete");
    }

    #[test]
    fn pasta_mudando_empurra_o_prazo() {
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::PastaMudou("a".into()), 0);
        sc.on_event(WatchEvent::PastaMudou("a".into()), 60);
        assert!(sc.due(150).is_empty());
        assert_eq!(sc.due(180), s(&["a"]));
    }

    #[test]
    fn com_jogo_aberto_pasta_so_marca() {
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::JogoAbriu("a".into()), 0);
        sc.on_event(WatchEvent::PastaMudou("a".into()), 10);
        assert!(sc.due(500).is_empty());
        sc.on_event(WatchEvent::JogoFechou("a".into()), 600);
        assert_eq!(sc.due(610), s(&["a"]));
    }

    #[test]
    fn abrir_cancela_agendamento_da_pasta() {
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::PastaMudou("a".into()), 0);
        sc.on_event(WatchEvent::JogoAbriu("a".into()), 30);
        assert!(sc.due(200).is_empty());
    }

    #[test]
    fn parar_sem_evento_de_saida_tambem_agenda() {
        // Crash: o jogo abre e o processo some sem nada no meio.
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::JogoAbriu("a".into()), 0);
        sc.on_event(WatchEvent::JogoFechou("a".into()), 5);
        assert_eq!(sc.due(15), s(&["a"]));
        assert!(sc.running().is_empty());
    }

    #[test]
    fn tempo_monotonico_nao_volta() {
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::JogoFechou("a".into()), 100);
        assert!(sc.due(50).is_empty());
        assert_eq!(sc.due(110), s(&["a"]));
    }

    #[test]
    fn running_e_pendentes() {
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::JogoAbriu("a".into()), 0);
        sc.on_event(WatchEvent::PastaMudou("b".into()), 0);
        assert_eq!(sc.running(), s(&["a"]));
        assert_eq!(sc.pending_count(), 1);
    }

    #[test]
    fn varios_jogos_vencem_em_ordem() {
        let mut sc = Scheduler::new();
        sc.on_event(WatchEvent::JogoFechou("b".into()), 0);
        sc.on_event(WatchEvent::JogoFechou("a".into()), 0);
        assert_eq!(sc.due(10), s(&["a", "b"]));
    }
}
