//! Teste de ponta a ponta com os dados reais deste PC (não altera os saves originais):
//! varre, faz backup de todos os jogos achados numa pasta temporária e restaura a
//! versão de cada um num "PC novo" simulado (outro usuário, dentro do tempdir).
//!
//! `SAVEGUARD_MANIFEST=<manifest.yaml> cargo test -p saveguard-core --release --test ponta_a_ponta_real -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::fs;

use chrono::Utc;
use saveguard_core::backup::{BackupOutcome, backup_game};
use saveguard_core::paths::{GameVars, PathContext};
use saveguard_core::restore::{apply_restore, plan_restore};
use saveguard_core::scan::{ScanOptions, scan};

#[test]
#[ignore]
fn backup_e_restauracao_com_saves_reais() {
    let path = std::env::var("SAVEGUARD_MANIFEST").expect("defina SAVEGUARD_MANIFEST");
    let manifest = saveguard_core::manifest::parse(&fs::read_to_string(path).unwrap()).unwrap();
    let ctx = PathContext::from_system();
    let installed = saveguard_core::stores::detect_all();
    let result = scan(&ctx, &manifest, &installed, &ScanOptions::default());
    assert!(!result.games.is_empty(), "nenhum jogo encontrado neste PC");

    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("Destino");
    fs::create_dir_all(&dest).unwrap();

    // "PC novo": mesmo layout, mas com o home dentro do tempdir.
    let novo_home = t.path().join("PCNovo/Users/Bruno");
    let rebase = |p: &std::path::Path| novo_home.join(p.strip_prefix(&ctx.home).unwrap_or(p));
    let novo = PathContext {
        app_data: rebase(&ctx.app_data),
        local_app_data: rebase(&ctx.local_app_data),
        local_app_data_low: rebase(&ctx.local_app_data_low),
        documents: rebase(&ctx.documents),
        public: t.path().join("PCNovo/Users/Public"),
        program_data: t.path().join("PCNovo/ProgramData"),
        os_user_name: "Bruno".into(),
        store_user_id: ctx.store_user_id.clone(),
        home: novo_home.clone(),
    };

    for g in &result.games {
        let out = backup_game(g, &ctx, &dest, 10, Utc::now()).unwrap();
        let BackupOutcome::Criado(r) = out else {
            panic!("{}: esperava versão nova", g.name)
        };
        let plan = plan_restore(&dest, &g.id, &r.stem, &novo, &GameVars::default()).unwrap();
        let rep = apply_restore(&plan, &dest, &novo, &BTreeMap::new(), Utc::now()).unwrap();
        let resolvidos = plan.items.iter().filter(|i| i.target.is_some()).count();
        assert_eq!(rep.restored, resolvidos, "{}", g.name);
        // Nada pode ser escrito fora do PC simulado.
        for target in plan.items.iter().filter_map(|i| i.target.as_ref()) {
            assert!(
                target.starts_with(t.path()),
                "{} saiu do PC simulado: {target:?}",
                g.name
            );
        }
        let originais: u64 = g.files.iter().map(|f| fs::metadata(f).unwrap().len()).sum();
        let restaurados: u64 = plan
            .items
            .iter()
            .filter_map(|i| i.target.as_ref())
            .map(|p| fs::metadata(p).unwrap().len())
            .sum();
        eprintln!(
            "{} | {} arquivos | {} não resolvidos | {} B originais | {} B restaurados",
            g.name,
            g.files.len(),
            plan.unresolved,
            originais,
            restaurados
        );
        if plan.unresolved == 0 {
            assert_eq!(originais, restaurados, "{}", g.name);
        }
    }
}
