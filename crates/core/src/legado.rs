//! Migração do nome antigo do app (SaveGuard, versão 0.1.0) para SavesGG.
//!
//! Nada é apagado: pastas de backup são renomeadas no mesmo lugar (operação atômica no
//! mesmo disco) e a configuração é copiada. Se o nome novo já existe, o antigo fica como
//! está para o usuário decidir.

use std::fs;
use std::path::Path;

use crate::backup::backup_root;
use crate::destinations::PASTA_NA_NUVEM;

/// Pasta de backup do 0.1.0 dentro de um destino.
pub const PASTA_ANTIGA: &str = "SaveGuard";
/// Subpasta do 0.1.0 dentro das pastas sincronizadas.
pub const PASTA_NA_NUVEM_ANTIGA: &str = "SaveGuard Backups";
/// Pasta do 0.1.0 em `%APPDATA%` e `%LOCALAPPDATA%`.
pub const PASTA_DO_APP_ANTIGA: &str = "SaveGuard";

/// Renomeia `antigo` para `novo` quando só o antigo existe. Devolve se renomeou.
fn renomear_se_so_antigo(antigo: &Path, novo: &Path) -> bool {
    antigo.is_dir() && !novo.exists() && fs::rename(antigo, novo).is_ok()
}

/// `<destino>\SaveGuard` → `<destino>\SavesGG` (pendrive, HD, pasta escolhida, fila).
pub fn migrar_destino(dest_root: &Path) -> bool {
    renomear_se_so_antigo(&dest_root.join(PASTA_ANTIGA), &backup_root(dest_root))
}

/// `<nuvem>\SaveGuard Backups\SaveGuard` → `<nuvem>\SavesGG Backups\SavesGG`.
pub fn migrar_pasta_sincronizada(sync_root: &Path) -> bool {
    let movida = renomear_se_so_antigo(
        &sync_root.join(PASTA_NA_NUVEM_ANTIGA),
        &sync_root.join(PASTA_NA_NUVEM),
    );
    let dentro = migrar_destino(&sync_root.join(PASTA_NA_NUVEM));
    movida || dentro
}

/// Copia `config.json` e `manifest.yaml` de `%APPDATA%\SaveGuard` para a pasta nova,
/// só quando ainda não existem lá. O original fica (o usuário desinstala o 0.1.0).
pub fn migrar_configuracao(pasta_antiga: &Path, pasta_nova: &Path) -> usize {
    let mut copiados = 0;
    for nome in ["config.json", "manifest.yaml"] {
        let de = pasta_antiga.join(nome);
        let para = pasta_nova.join(nome);
        if de.is_file()
            && !para.exists()
            && fs::create_dir_all(pasta_nova).is_ok()
            && fs::copy(&de, &para).is_ok()
        {
            copiados += 1;
        }
    }
    copiados
}

/// Move a fila `%LOCALAPPDATA%\SaveGuard\pendentes` para a pasta nova. Só a fila: o
/// resto de `%LOCALAPPDATA%\SaveGuard` é a instalação do 0.1.0.
/// Também ajusta a pasta interna de cada destino da fila (`<hash>\SaveGuard`).
pub fn migrar_fila(pasta_antiga: &Path, pasta_nova: &Path) -> bool {
    let novo = pasta_nova.join("pendentes");
    let movida = !novo.exists()
        && pasta_antiga.join("pendentes").is_dir()
        && fs::create_dir_all(pasta_nova).is_ok()
        && renomear_se_so_antigo(&pasta_antiga.join("pendentes"), &novo);
    if let Ok(dirs) = fs::read_dir(&novo) {
        for d in dirs.flatten() {
            migrar_destino(&d.path());
        }
    }
    movida
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indice(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("biblioteca.json"), "{}").unwrap();
    }

    #[test]
    fn destino_antigo_vira_novo() {
        let t = tempfile::tempdir().unwrap();
        indice(&t.path().join("SaveGuard"));
        assert!(migrar_destino(t.path()));
        assert!(t.path().join("SavesGG/biblioteca.json").is_file());
        assert!(!t.path().join("SaveGuard").exists());
        assert!(!migrar_destino(t.path()), "segunda vez não faz nada");
    }

    #[test]
    fn destino_com_os_dois_nomes_nao_mexe() {
        let t = tempfile::tempdir().unwrap();
        indice(&t.path().join("SaveGuard"));
        indice(&t.path().join("SavesGG"));
        assert!(!migrar_destino(t.path()));
        assert!(t.path().join("SaveGuard/biblioteca.json").is_file());
    }

    #[test]
    fn pasta_sincronizada_migra_as_duas_pastas() {
        let t = tempfile::tempdir().unwrap();
        indice(&t.path().join("SaveGuard Backups/SaveGuard"));
        assert!(migrar_pasta_sincronizada(t.path()));
        assert!(
            t.path()
                .join("SavesGG Backups/SavesGG/biblioteca.json")
                .is_file()
        );
        assert!(!t.path().join("SaveGuard Backups").exists());
    }

    #[test]
    fn pasta_sincronizada_sem_nada_antigo() {
        let t = tempfile::tempdir().unwrap();
        assert!(!migrar_pasta_sincronizada(t.path()));
        assert!(!t.path().join("SavesGG Backups").exists());
    }

    #[test]
    fn configuracao_copiada_sem_sobrescrever() {
        let t = tempfile::tempdir().unwrap();
        let antiga = t.path().join("SaveGuard");
        let nova = t.path().join("SavesGG");
        fs::create_dir_all(&antiga).unwrap();
        fs::write(antiga.join("config.json"), r#"{"theme":"cyberpunk"}"#).unwrap();
        fs::write(antiga.join("manifest.yaml"), "a: 1").unwrap();
        assert_eq!(migrar_configuracao(&antiga, &nova), 2);
        assert!(antiga.join("config.json").is_file(), "original fica");
        fs::write(nova.join("config.json"), r#"{"theme":"pastel"}"#).unwrap();
        assert_eq!(migrar_configuracao(&antiga, &nova), 0);
        assert!(
            fs::read_to_string(nova.join("config.json"))
                .unwrap()
                .contains("pastel")
        );
    }

    #[test]
    fn fila_movida_e_instalacao_fica() {
        let t = tempfile::tempdir().unwrap();
        let antiga = t.path().join("SaveGuard");
        let nova = t.path().join("SavesGG");
        indice(&antiga.join("pendentes/abc/SaveGuard"));
        fs::write(antiga.join("saveguard.exe"), "exe").unwrap();
        assert!(migrar_fila(&antiga, &nova));
        assert!(nova.join("pendentes/abc/SavesGG/biblioteca.json").is_file());
        assert!(antiga.join("saveguard.exe").is_file());
        assert!(!migrar_fila(&antiga, &nova));
    }
}
