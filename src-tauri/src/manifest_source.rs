//! De onde vem o manifesto: cópia baixada (atualizada semanalmente) ou a embutida no instalador.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use saveguard_core::manifest::{Manifest, parse};

pub const URL: &str =
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
const SEMANA: Duration = Duration::from_secs(7 * 24 * 3600);

/// A cópia baixada precisa ser atualizada?
pub fn needs_refresh(modified: Option<SystemTime>, now: SystemTime) -> bool {
    match modified {
        None => true,
        Some(m) => now.duration_since(m).map(|d| d > SEMANA).unwrap_or(false),
    }
}

fn download(to: &Path) -> Result<(), String> {
    let body = ureq::get(URL)
        .config()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(100 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    // Só troca a cópia local se o conteúdo baixado for um manifesto válido.
    parse(&body).map_err(|e| e.to_string())?;
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = to.with_extension("yaml.tmp");
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, to).map_err(|e| e.to_string())
}

/// Carrega o manifesto: tenta atualizar a cópia baixada (se tiver mais de uma semana);
/// se não der, usa a cópia baixada antiga ou a embutida.
pub fn load(cache: &Path, bundled: &[PathBuf]) -> Result<Manifest, String> {
    let modified = cache.metadata().and_then(|m| m.modified()).ok();
    if needs_refresh(modified, SystemTime::now()) {
        let _ = download(cache);
    }
    let candidates = std::iter::once(cache.to_path_buf()).chain(bundled.iter().cloned());
    for path in candidates {
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(m) = parse(&text)
        {
            return Ok(m);
        }
    }
    Err("manifesto de saves indisponível".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_copia_precisa_baixar() {
        assert!(needs_refresh(None, SystemTime::now()));
    }

    #[test]
    fn copia_de_3_dias_serve() {
        let now = SystemTime::now();
        assert!(!needs_refresh(
            Some(now - Duration::from_secs(3 * 86400)),
            now
        ));
    }

    #[test]
    fn copia_de_8_dias_atualiza() {
        let now = SystemTime::now();
        assert!(needs_refresh(
            Some(now - Duration::from_secs(8 * 86400)),
            now
        ));
    }

    #[test]
    fn usa_embutido_quando_cache_falta() {
        let t = tempfile::tempdir().unwrap();
        let bundled = t.path().join("manifest.yaml");
        std::fs::write(
            &bundled,
            "Jogo:\n  files:\n    \"<base>/s.sav\":\n      tags: [save]\n",
        )
        .unwrap();
        // cache recente e inválido: não tenta baixar e cai no embutido
        let cache = t.path().join("cache.yaml");
        std::fs::write(&cache, "isso: [quebrado").unwrap();
        let m = load(&cache, &[bundled]).unwrap();
        assert!(m.games.contains_key("Jogo"));
    }
}
