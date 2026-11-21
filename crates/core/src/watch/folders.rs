//! Qual pasta de save pertence a qual jogo, e quais pastas vigiar.

use std::path::{Path, PathBuf};

use crate::paths::norm;
use crate::scan::DetectedGame;

fn inside(path: &str, root: &str) -> bool {
    path == root || path.starts_with(&format!("{root}/"))
}

/// Jogos cujas pastas de save contêm `path`.
pub fn games_for_path(path: &Path, games: &[DetectedGame]) -> Vec<String> {
    let p = norm(path).to_lowercase();
    games
        .iter()
        .filter(|g| {
            g.save_roots
                .iter()
                .any(|r| inside(&p, &norm(r).to_lowercase()))
        })
        .map(|g| g.id.clone())
        .collect()
}

/// Pastas a vigiar: as de save, sem repetidas e sem as que já estão dentro de outra.
pub fn watch_roots(games: &[DetectedGame]) -> Vec<PathBuf> {
    let mut all: Vec<(String, PathBuf)> = games
        .iter()
        .flat_map(|g| g.save_roots.iter())
        .map(|r| (norm(r).to_lowercase(), r.clone()))
        .collect();
    all.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(&b.0)));
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for (k, p) in all {
        if !out.iter().any(|(o, _)| inside(&k, o)) {
            out.push((k, p));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::process::tests::game;
    use std::path::PathBuf;

    #[test]
    fn pasta_pertence_ao_jogo() {
        let games = [
            game("x", None, None, "C:/Saves/X"),
            game("y", None, None, "C:/Saves/Y"),
        ];
        assert_eq!(
            games_for_path(Path::new(r"c:\saves\x\sub\a.sav"), &games),
            vec!["x".to_string()]
        );
        assert!(games_for_path(Path::new("C:/Saves/XY/a.sav"), &games).is_empty());
        assert!(games_for_path(Path::new("D:/outro"), &games).is_empty());
    }

    #[test]
    fn watch_roots_sem_repetidos() {
        let games = [
            game("a", None, None, "C:/A"),
            game("b", None, None, "C:/A/B"),
            game("c", None, None, "C:/c"),
            game("d", None, None, "c:/a"),
        ];
        assert_eq!(
            watch_roots(&games),
            vec![PathBuf::from("C:/A"), PathBuf::from("C:/c")]
        );
    }
}
