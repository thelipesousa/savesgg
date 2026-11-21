//! O que já protege cada jogo e qual a prioridade de backup.

pub use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::manifest::Store;
use crate::paths::norm;
use crate::scan::DetectedGame;
use crate::sync_folders::{SyncProvider, SyncRoot};

/// Backup mais recente que o próprio app fez de um jogo.
pub trait BackupHistory {
    fn last_backup(&self, game_id: &str) -> Option<DateTime<Utc>>;
}

/// Dias até um backup do app deixar de contar como "recente".
pub const DIAS_BACKUP_RECENTE: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "tipo", content = "valor", rename_all = "snake_case")]
pub enum Protection {
    NuvemDaLoja(Store),
    /// O manifesto diz que o jogo tem nuvem, mas em outra loja (ou sem loja).
    NuvemNaoConfirmada,
    PastaSincronizada(SyncProvider),
    BackupDoApp(DateTime<Utc>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Alta,
    Media,
    Baixa,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assessment {
    pub priority: Priority,
    pub protections: Vec<Protection>,
}

fn inside(path: &std::path::Path, root: &std::path::Path) -> bool {
    let p = norm(path).to_lowercase();
    let r = norm(root).to_lowercase();
    !r.is_empty() && (p == r || p.starts_with(&format!("{r}/")))
}

pub fn assess(
    game: &DetectedGame,
    sync_roots: &[SyncRoot],
    history: &dyn BackupHistory,
    now: DateTime<Utc>,
) -> Assessment {
    let c = game.cloud;
    let any_cloud = c.steam || c.epic || c.gog || c.origin || c.uplay;
    let confirmed = match game.store {
        Store::Steam => c.steam,
        Store::Epic => c.epic,
        Store::Gog => c.gog,
        Store::Xbox => true,
        Store::Emulador | Store::SemLoja => false,
    };
    let mut protections = Vec::new();
    let mut baixa = false;
    let mut media = false;
    if confirmed {
        protections.push(Protection::NuvemDaLoja(game.store));
        baixa = true;
    } else if any_cloud {
        protections.push(Protection::NuvemNaoConfirmada);
        media = true;
    }
    if !game.save_roots.is_empty()
        && let Some(sr) = sync_roots
            .iter()
            .find(|sr| game.save_roots.iter().all(|r| inside(r, &sr.path)))
    {
        protections.push(Protection::PastaSincronizada(sr.provider));
        baixa = true;
    }
    if let Some(last) = history.last_backup(&game.id) {
        protections.push(Protection::BackupDoApp(last));
        if now - last <= chrono::Duration::days(DIAS_BACKUP_RECENTE) {
            baixa = true;
        } else {
            media = true;
        }
    }
    let priority = if baixa {
        Priority::Baixa
    } else if media {
        Priority::Media
    } else {
        Priority::Alta
    };
    Assessment {
        priority,
        protections,
    }
}

/// Alta → Média → Baixa; dentro de cada uma, por nome.
pub fn sort_for_display(items: &mut [(DetectedGame, Assessment)]) {
    items.sort_by_key(|(g, a)| (a.priority, g.name.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{CloudFlags, Store};
    use crate::scan::{DetectedGame, Source};
    use crate::sync_folders::{SyncProvider, SyncRoot};
    use chrono::{Duration, TimeZone, Utc};
    use std::collections::HashMap;
    use std::path::PathBuf;

    struct Hist(HashMap<String, DateTime<Utc>>);
    impl BackupHistory for Hist {
        fn last_backup(&self, game_id: &str) -> Option<DateTime<Utc>> {
            self.0.get(game_id).copied()
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap()
    }

    fn game(store: Store, cloud: CloudFlags, root: &str) -> DetectedGame {
        DetectedGame {
            id: "jogo".into(),
            name: "Jogo".into(),
            store,
            save_roots: vec![PathBuf::from(root)],
            files: vec![PathBuf::from(root).join("a.sav")],
            total_bytes: 1,
            cloud,
            source: Source::Manifesto,
            exe_hint: None,
            base: None,
            root: None,
            store_game_id: None,
            patterns: Vec::new(),
            steam_app_id: None,
        }
    }

    fn nada() -> Hist {
        Hist(HashMap::new())
    }

    const ROOT: &str = r"C:\Users\João\Saved Games\Jogo";

    #[test]
    fn sem_nada_e_alta() {
        let a = assess(
            &game(Store::SemLoja, CloudFlags::default(), ROOT),
            &[],
            &nada(),
            now(),
        );
        assert_eq!(a.priority, Priority::Alta);
        assert!(a.protections.is_empty());
    }

    #[test]
    fn steam_com_cloud_e_baixa() {
        let cloud = CloudFlags {
            steam: true,
            ..Default::default()
        };
        let a = assess(&game(Store::Steam, cloud, ROOT), &[], &nada(), now());
        assert_eq!(a.priority, Priority::Baixa);
        assert_eq!(a.protections, vec![Protection::NuvemDaLoja(Store::Steam)]);
    }

    #[test]
    fn gog_com_flag_so_steam_e_media() {
        let cloud = CloudFlags {
            steam: true,
            ..Default::default()
        };
        let a = assess(&game(Store::Gog, cloud, ROOT), &[], &nada(), now());
        assert_eq!(a.priority, Priority::Media);
        assert_eq!(a.protections, vec![Protection::NuvemNaoConfirmada]);
    }

    #[test]
    fn sem_loja_com_flag_e_media() {
        let cloud = CloudFlags {
            epic: true,
            ..Default::default()
        };
        let a = assess(&game(Store::SemLoja, cloud, ROOT), &[], &nada(), now());
        assert_eq!(a.priority, Priority::Media);
    }

    #[test]
    fn xbox_e_sempre_confirmado() {
        let a = assess(
            &game(Store::Xbox, CloudFlags::default(), ROOT),
            &[],
            &nada(),
            now(),
        );
        assert_eq!(a.priority, Priority::Baixa);
    }

    #[test]
    fn dentro_do_onedrive_e_baixa() {
        let g = game(
            Store::SemLoja,
            CloudFlags::default(),
            r"C:\Users\João\OneDrive\Documentos\My Games\X",
        );
        let roots = [SyncRoot {
            provider: SyncProvider::OneDrive,
            path: PathBuf::from(r"c:\users\joão\onedrive"),
        }];
        let a = assess(&g, &roots, &nada(), now());
        assert_eq!(a.priority, Priority::Baixa);
        assert_eq!(
            a.protections,
            vec![Protection::PastaSincronizada(SyncProvider::OneDrive)]
        );
    }

    #[test]
    fn prefixo_parecido_nao_conta() {
        let g = game(
            Store::SemLoja,
            CloudFlags::default(),
            r"C:\Users\João\OneDrive2\X",
        );
        let roots = [SyncRoot {
            provider: SyncProvider::OneDrive,
            path: PathBuf::from(r"C:\Users\João\OneDrive"),
        }];
        assert_eq!(assess(&g, &roots, &nada(), now()).priority, Priority::Alta);
    }

    #[test]
    fn backup_8_dias_e_media() {
        let h = Hist(HashMap::from([(
            "jogo".to_string(),
            now() - Duration::days(8),
        )]));
        let a = assess(
            &game(Store::SemLoja, CloudFlags::default(), ROOT),
            &[],
            &h,
            now(),
        );
        assert_eq!(a.priority, Priority::Media);
    }

    #[test]
    fn backup_6_dias_e_baixa() {
        let h = Hist(HashMap::from([(
            "jogo".to_string(),
            now() - Duration::days(6),
        )]));
        let a = assess(
            &game(Store::SemLoja, CloudFlags::default(), ROOT),
            &[],
            &h,
            now(),
        );
        assert_eq!(a.priority, Priority::Baixa);
        assert_eq!(
            a.protections,
            vec![Protection::BackupDoApp(now() - Duration::days(6))]
        );
    }

    #[test]
    fn ordena_alta_primeiro() {
        let mk = |name: &str, p: Priority| {
            let mut g = game(Store::SemLoja, CloudFlags::default(), ROOT);
            g.name = name.into();
            (
                g,
                Assessment {
                    priority: p,
                    protections: vec![],
                },
            )
        };
        let mut v = vec![
            mk("b", Priority::Baixa),
            mk("Zeta", Priority::Alta),
            mk("m", Priority::Media),
            mk("alfa", Priority::Alta),
        ];
        sort_for_display(&mut v);
        let names: Vec<_> = v.iter().map(|(g, _)| g.name.as_str()).collect();
        assert_eq!(names, ["alfa", "Zeta", "m", "b"]);
    }
}
