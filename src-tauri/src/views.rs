//! Conversão dos dados do motor para o formato que a interface recebe.

use chrono::{DateTime, Utc};
use serde::Serialize;

use savesgg_core::manifest::Store;
use savesgg_core::paths::{PathContext, contract};
use savesgg_core::protection::{Assessment, Priority, Protection};
use savesgg_core::scan::{DetectedGame, Source};
use savesgg_core::sync_folders::SyncProvider;

/// Um jogo como a interface mostra. Caminhos vão com marcadores (sem nome de usuário).
#[derive(Debug, Clone, Serialize)]
pub struct GameView {
    pub id: String,
    pub name: String,
    pub store: Store,
    pub source: Source,
    pub priority: Priority,
    pub protections: Vec<String>,
    pub files: usize,
    pub total_bytes: u64,
    pub last_backup: Option<String>,
    pub save_roots: Vec<String>,
    /// Acima do limite de 2 GB: pede confirmação.
    pub grande: bool,
    /// Possível save (heurística) já confirmado pelo usuário.
    pub confirmado: bool,
    /// Capa no CDN da Steam (a interface cai no cartão com a inicial se falhar).
    pub cover_url: Option<String>,
}

fn provider_name(p: SyncProvider) -> &'static str {
    match p {
        SyncProvider::OneDrive => "OneDrive",
        SyncProvider::GoogleDrive => "Google Drive",
        SyncProvider::Dropbox => "Dropbox",
    }
}

pub fn protection_label(p: &Protection) -> String {
    match p {
        Protection::NuvemDaLoja(s) => match s {
            Store::Steam => "Steam Cloud",
            Store::Epic => "Epic Cloud",
            Store::Gog => "GOG Cloud",
            Store::Xbox => "Xbox Cloud",
            Store::Emulador | Store::SemLoja => "Nuvem da loja",
        }
        .to_string(),
        Protection::NuvemNaoConfirmada => "Nuvem não confirmada".to_string(),
        Protection::PastaSincronizada(p) => format!("Pasta do {}", provider_name(*p)),
        Protection::BackupDoApp(_) => "Backup do SavesGG".to_string(),
    }
}

pub fn game_view(
    g: &DetectedGame,
    a: &Assessment,
    ctx: &PathContext,
    last_backup: Option<DateTime<Utc>>,
    grande: bool,
    confirmado: bool,
) -> GameView {
    GameView {
        id: g.id.clone(),
        name: g.name.clone(),
        store: g.store,
        source: g.source,
        priority: a.priority,
        protections: a.protections.iter().map(protection_label).collect(),
        files: g.files.len(),
        total_bytes: g.total_bytes,
        last_backup: last_backup.map(|d| d.to_rfc3339()),
        save_roots: g.save_roots.iter().map(|r| contract(r, ctx)).collect(),
        grande,
        confirmado,
        cover_url: g.steam_app_id.map(|id| {
            format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/library_600x900.jpg")
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use savesgg_core::manifest::{CloudFlags, Store};
    use savesgg_core::paths::PathContext;
    use savesgg_core::protection::{Assessment, Priority, Protection};
    use savesgg_core::scan::{DetectedGame, Source};
    use savesgg_core::sync_folders::SyncProvider;
    use std::path::PathBuf;

    fn ctx() -> PathContext {
        let home = PathBuf::from(r"C:\Users\Ana");
        PathContext {
            app_data: home.join(r"AppData\Roaming"),
            local_app_data: home.join(r"AppData\Local"),
            local_app_data_low: home.join(r"AppData\LocalLow"),
            documents: home.join("Documents"),
            public: PathBuf::from(r"C:\Users\Public"),
            program_data: PathBuf::from(r"C:\ProgramData"),
            os_user_name: "Ana".into(),
            store_user_id: None,
            home,
        }
    }

    fn game() -> DetectedGame {
        let root = PathBuf::from(r"C:\Users\Ana\AppData\Roaming\EldenRing");
        DetectedGame {
            id: "elden-ring".into(),
            name: "Elden Ring".into(),
            store: Store::Steam,
            files: vec![root.join("a.sl2"), root.join("b.sl2")],
            save_roots: vec![root],
            total_bytes: 2048,
            cloud: CloudFlags::default(),
            source: Source::Manifesto,
            exe_hint: None,
            base: None,
            root: None,
            store_game_id: None,
            patterns: Vec::new(),
            steam_app_id: None,
        }
    }

    #[test]
    fn game_view_contrai_caminhos() {
        let a = Assessment {
            priority: Priority::Alta,
            protections: vec![],
        };
        let v = game_view(&game(), &a, &ctx(), None, false, false);
        assert_eq!(v.save_roots, vec!["<winAppData>/EldenRing".to_string()]);
        assert_eq!(v.files, 2);
        assert_eq!(v.priority, Priority::Alta);
        assert!(!v.grande);
    }

    #[test]
    fn rotulos_de_protecao_em_portugues() {
        let d = Utc.with_ymd_and_hms(2026, 11, 20, 0, 0, 0).unwrap();
        assert_eq!(
            protection_label(&Protection::NuvemDaLoja(Store::Steam)),
            "Steam Cloud"
        );
        assert_eq!(
            protection_label(&Protection::NuvemNaoConfirmada),
            "Nuvem não confirmada"
        );
        assert_eq!(
            protection_label(&Protection::PastaSincronizada(SyncProvider::OneDrive)),
            "Pasta do OneDrive"
        );
        assert_eq!(
            protection_label(&Protection::BackupDoApp(d)),
            "Backup do SavesGG"
        );
    }

    #[test]
    fn game_view_tem_capa() {
        let a = Assessment {
            priority: Priority::Alta,
            protections: vec![],
        };
        let mut g = game();
        g.steam_app_id = Some(1245620);
        let v = game_view(&g, &a, &ctx(), None, false, false);
        assert!(
            v.cover_url
                .as_deref()
                .unwrap()
                .ends_with("/1245620/library_600x900.jpg"),
            "{:?}",
            v.cover_url
        );
        g.steam_app_id = None;
        assert_eq!(
            game_view(&g, &a, &ctx(), None, false, false).cover_url,
            None
        );
    }

    #[test]
    fn last_backup_em_rfc3339() {
        let d = Utc.with_ymd_and_hms(2026, 11, 20, 21, 40, 5).unwrap();
        let a = Assessment {
            priority: Priority::Baixa,
            protections: vec![],
        };
        let v = game_view(&game(), &a, &ctx(), Some(d), true, true);
        assert_eq!(v.last_backup.as_deref(), Some("2026-11-20T21:40:05+00:00"));
        assert!(v.grande && v.confirmado);
    }
}
