//! Pendrive/HD externo plugado ou removido: avisa e oferece copiar tudo.

use std::collections::BTreeSet;

use tauri::{AppHandle, Emitter, Manager};

use saveguard_core::destinations::{Destination, DestinationKind};

use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveEvent {
    Plugado(Destination),
    Removido(String),
}

/// Compara os discos externos de agora com os da última leitura.
#[derive(Debug, Default)]
pub struct DriveWatcher {
    /// `None` antes da primeira leitura (discos já plugados ao abrir não avisam).
    known: Option<BTreeSet<String>>,
}

impl DriveWatcher {
    pub fn update(&mut self, all: &[Destination]) -> Vec<DriveEvent> {
        let now: Vec<&Destination> = all
            .iter()
            .filter(|d| d.kind == DestinationKind::DiscoRemovivel && d.available)
            .collect();
        let ids: BTreeSet<String> = now.iter().map(|d| d.id.clone()).collect();
        let Some(known) = self.known.replace(ids.clone()) else {
            return Vec::new();
        };
        let mut ev: Vec<DriveEvent> = now
            .iter()
            .filter(|d| !known.contains(&d.id))
            .map(|d| DriveEvent::Plugado((*d).clone()))
            .collect();
        ev.extend(
            known
                .difference(&ids)
                .map(|id| DriveEvent::Removido(id.clone())),
        );
        ev
    }
}

/// Texto da notificação de disco plugado.
pub fn aviso(d: &Destination) -> String {
    if d.has_backup {
        format!(
            "{} conectado — abra o SaveGuard para atualizar o backup.",
            d.label
        )
    } else {
        format!(
            "{} conectado — abra o SaveGuard para copiar seus saves.",
            d.label
        )
    }
}

/// Chamado pelo motor a cada volta: avisa discos novos e atualiza a bandeja.
pub fn poll(app: &AppHandle, dests: &[Destination]) {
    let state = app.state::<AppState>();
    let events = state
        .drive_watcher
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .update(dests);
    // A lista do menu acompanha os discos de agora, inclusive os que já estavam
    // plugados quando o app abriu (esses não geram aviso).
    let plugged: Vec<Destination> = dests
        .iter()
        .filter(|d| d.kind == DestinationKind::DiscoRemovivel && d.available)
        .cloned()
        .collect();
    let changed = {
        let mut cur = state.plugged.lock().unwrap_or_else(|e| e.into_inner());
        let changed = *cur != plugged;
        *cur = plugged;
        changed
    };
    if events.is_empty() && !changed {
        return;
    }
    for ev in &events {
        match ev {
            DriveEvent::Plugado(d) => {
                crate::notify_user(app, "Disco conectado", &aviso(d));
                let _ = app.emit("disco-plugado", d);
            }
            DriveEvent::Removido(id) => {
                let _ = app.emit("disco-removido", id);
            }
        }
    }
    let status = state
        .auto_status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    crate::tray::update(app, &status);
}

#[cfg(test)]
mod tests {
    use super::*;
    use saveguard_core::destinations::{Destination, DestinationKind};
    use std::path::PathBuf;

    fn disco(id: &str) -> Destination {
        Destination {
            id: id.into(),
            label: format!("PEN ({id})"),
            kind: DestinationKind::DiscoRemovivel,
            root: PathBuf::from("E:/"),
            available: true,
            free_bytes: None,
            has_backup: false,
        }
    }

    #[test]
    fn primeira_leitura_nao_avisa() {
        let mut w = DriveWatcher::default();
        assert!(w.update(&[disco("disco:1")]).is_empty());
        assert!(w.update(&[disco("disco:1")]).is_empty());
    }

    #[test]
    fn plugar_e_remover() {
        let mut w = DriveWatcher::default();
        assert!(w.update(&[]).is_empty());
        let ev = w.update(&[disco("disco:1")]);
        assert!(
            matches!(&ev[..], [DriveEvent::Plugado(d)] if d.id == "disco:1"),
            "{ev:?}"
        );
        let ev = w.update(&[]);
        assert_eq!(ev, vec![DriveEvent::Removido("disco:1".into())]);
    }

    #[test]
    fn ignora_nuvem_e_pastas() {
        let mut w = DriveWatcher::default();
        w.update(&[]);
        let mut nuvem = disco("sync:x");
        nuvem.kind = DestinationKind::PastaLocal;
        assert!(w.update(&[nuvem]).is_empty());
    }

    #[test]
    fn texto_do_aviso() {
        let mut d = disco("disco:1");
        assert!(aviso(&d).contains("copiar seus saves"));
        d.has_backup = true;
        assert!(aviso(&d).contains("atualizar o backup"));
    }
}
