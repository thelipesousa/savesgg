//! Validação dos nomes que vêm da nuvem, antes de virarem caminho no PC
//! (protege contra travessia de caminho e arquivos estranhos).

/// Id de jogo: `[a-z0-9-]`, de 1 a 100 caracteres.
pub fn jogo_valido(s: &str) -> bool {
    (1..=100).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Nome de versão: `AAAA-MM-DD_HH-MM-SS`, com sufixo opcional `_N` (o que o motor de
/// backup usa quando duas versões caem no mesmo segundo) ou `-N`.
pub fn versao_valida(stem: &str) -> bool {
    let b = stem.as_bytes();
    if b.len() < 19 {
        return false;
    }
    let base_ok = b[..19].iter().enumerate().all(|(i, c)| match i {
        4 | 7 | 13 | 16 => *c == b'-',
        10 => *c == b'_',
        _ => c.is_ascii_digit(),
    });
    let sufixo = &b[19..];
    base_ok
        && (sufixo.is_empty()
            || (sufixo.len() >= 2
                && (sufixo[0] == b'-' || sufixo[0] == b'_')
                && sufixo[1..].iter().all(u8::is_ascii_digit)))
}

/// Arquivo de versão vindo da nuvem: devolve o nome da versão e a extensão.
pub fn arquivo_remoto_valido(nome: &str) -> Option<(String, &'static str)> {
    let (stem, ext) = nome.rsplit_once('.')?;
    let ext = match ext {
        "zip" => "zip",
        "json" => "json",
        _ => return None,
    };
    versao_valida(stem).then(|| (stem.to_string(), ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jogo_valido_aceita_slug() {
        assert!(jogo_valido("elden-ring"));
        assert!(jogo_valido("hades-ii-1a2b"));
        assert!(!jogo_valido("../x"));
        assert!(!jogo_valido("Jogo X"));
        assert!(!jogo_valido(""));
        assert!(!jogo_valido(&"a".repeat(101)));
    }

    #[test]
    fn versao_valida_formato() {
        assert!(versao_valida("2026-11-20_21-40-05"));
        assert!(versao_valida("2026-11-20_21-40-05-2"));
        // o motor de backup usa `_N` quando duas versões caem no mesmo segundo
        assert!(versao_valida("2026-11-20_21-40-05_2"));
        assert!(!versao_valida("2026-11-20_21-40-05_"));
        assert!(!versao_valida("2026-11-20"));
        assert!(!versao_valida("../a"));
        assert!(!versao_valida("2026-11-20_21-40-05-"));
    }

    #[test]
    fn arquivo_remoto_so_zip_e_json() {
        assert_eq!(
            arquivo_remoto_valido("2026-11-20_21-40-05.zip"),
            Some(("2026-11-20_21-40-05".to_string(), "zip"))
        );
        assert_eq!(
            arquivo_remoto_valido("2026-11-20_21-40-05.json"),
            Some(("2026-11-20_21-40-05".to_string(), "json"))
        );
        assert_eq!(arquivo_remoto_valido("a.exe"), None);
        assert_eq!(arquivo_remoto_valido("../2026-11-20_21-40-05.json"), None);
    }
}
