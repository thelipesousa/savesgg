//! Marcadores de caminho (`<home>`, `<winAppData>`…) no formato do manifesto do Ludusavi.
//!
//! Os backups guardam caminhos com marcadores, nunca absolutos, para poderem ser
//! restaurados num PC com outro nome de usuário ou outra letra de disco.

use std::path::{Path, PathBuf};

/// Pastas do usuário atual, resolvidas pelas APIs de pastas conhecidas do Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathContext {
    pub home: PathBuf,
    pub app_data: PathBuf,
    pub local_app_data: PathBuf,
    pub local_app_data_low: PathBuf,
    pub documents: PathBuf,
    pub public: PathBuf,
    pub program_data: PathBuf,
    pub os_user_name: String,
    pub store_user_id: Option<String>,
}

/// Variáveis específicas de um jogo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameVars {
    /// Pasta de instalação do jogo.
    pub base: Option<PathBuf>,
    /// Raiz da loja (ex.: pasta do Steam, onde fica `userdata`).
    pub root: Option<PathBuf>,
    pub game: Option<String>,
    pub store_game_id: Option<String>,
}

impl PathContext {
    /// Monta o contexto do usuário atual usando as pastas conhecidas do sistema
    /// (respeita Documentos redirecionados, por exemplo para o OneDrive).
    pub fn from_system() -> PathContext {
        let home = dirs::home_dir().unwrap_or_default();
        let app_data = dirs::data_dir().unwrap_or_else(|| home.join(r"AppData\Roaming"));
        let local_app_data = dirs::data_local_dir().unwrap_or_else(|| home.join(r"AppData\Local"));
        let local_app_data_low = local_app_data
            .parent()
            .map(|p| p.join("LocalLow"))
            .unwrap_or_else(|| home.join(r"AppData\LocalLow"));
        PathContext {
            documents: dirs::document_dir().unwrap_or_else(|| home.join("Documents")),
            public: dirs::public_dir().unwrap_or_else(|| PathBuf::from(r"C:\Users\Public")),
            program_data: std::env::var_os("PROGRAMDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")),
            os_user_name: std::env::var("USERNAME").unwrap_or_default(),
            store_user_id: None,
            app_data,
            local_app_data,
            local_app_data_low,
            home,
        }
    }

    /// Pares (marcador, pasta) do mais específico para o mais genérico.
    fn prefixes(&self) -> Vec<(&'static str, String)> {
        let mut v = vec![
            ("<winLocalAppDataLow>", norm(&self.local_app_data_low)),
            ("<winLocalAppData>", norm(&self.local_app_data)),
            ("<winAppData>", norm(&self.app_data)),
            ("<winDocuments>", norm(&self.documents)),
            ("<winPublic>", norm(&self.public)),
            ("<winProgramData>", norm(&self.program_data)),
            ("<home>", norm(&self.home)),
        ];
        v.retain(|(_, p)| !p.is_empty());
        v.sort_by_key(|(_, p)| std::cmp::Reverse(p.chars().count()));
        v
    }
}

/// Diferença entre o SteamID64 e o SteamID3 (ID de conta).
pub const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// SteamID64 a partir do SteamID3 (`"64734272"` → `"76561198025000000"`).
pub fn steam_id64(id3: &str) -> Option<String> {
    id3.parse::<u64>()
        .ok()
        .and_then(|n| n.checked_add(STEAM_ID64_BASE))
        .map(|n| n.to_string())
}

/// Caminho com `/` e sem barra final.
pub fn norm(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

/// Expande os marcadores de `template`. Devolve um padrão (pode conter glob) com `/`,
/// ou `None` se algum marcador exigido não estiver disponível.
pub fn expand(template: &str, ctx: &PathContext, vars: &GameVars) -> Option<String> {
    let mut out = template.replace('\\', "/");
    let base = vars.base.as_deref().map(norm);
    let root = vars.root.as_deref().map(norm);
    let id64 = ctx.store_user_id.as_deref().and_then(steam_id64);
    let fixed: [(&str, Option<String>); 14] = [
        ("<winLocalAppDataLow>", Some(norm(&ctx.local_app_data_low))),
        ("<winLocalAppData>", Some(norm(&ctx.local_app_data))),
        ("<winAppData>", Some(norm(&ctx.app_data))),
        ("<winDocuments>", Some(norm(&ctx.documents))),
        ("<winPublic>", Some(norm(&ctx.public))),
        ("<winProgramData>", Some(norm(&ctx.program_data))),
        ("<home>", Some(norm(&ctx.home))),
        ("<osUserName>", Some(ctx.os_user_name.clone())),
        ("<storeUserId64>", id64),
        ("<storeUserId>", ctx.store_user_id.clone()),
        ("<base>", base),
        ("<root>", root),
        ("<game>", vars.game.clone()),
        ("<storeGameId>", vars.store_game_id.clone()),
    ];
    for (marker, value) in fixed {
        if out.contains(marker) {
            out = out.replace(marker, &value?);
        }
    }
    Some(out)
}

/// Troca o prefixo de pasta conhecida mais longo pelo marcador correspondente.
/// A comparação ignora maiúsculas/minúsculas, como o Windows.
pub fn contract(path: &Path, ctx: &PathContext) -> String {
    contract_with(path, ctx, &GameVars::default())
}

/// Como [`contract`], considerando também `<base>` (pasta do jogo) e `<root>` (raiz da loja).
pub fn contract_with(path: &Path, ctx: &PathContext, vars: &GameVars) -> String {
    let p = norm(path);
    let chars: Vec<char> = p.chars().collect();
    let mut prefixes = ctx.prefixes();
    if let Some(b) = &vars.base {
        prefixes.push(("<base>", norm(b)));
    }
    if let Some(r) = &vars.root {
        prefixes.push(("<root>", norm(r)));
    }
    prefixes.retain(|(_, p)| !p.is_empty());
    prefixes.sort_by_key(|(_, p)| std::cmp::Reverse(p.chars().count()));
    for (marker, prefix) in prefixes {
        let pre: Vec<char> = prefix.chars().collect();
        if chars.len() < pre.len() {
            continue;
        }
        let same = chars[..pre.len()]
            .iter()
            .zip(&pre)
            .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()));
        let boundary = chars.len() == pre.len() || chars[pre.len()] == '/';
        if same && boundary {
            let rest: String = chars[pre.len()..].iter().collect();
            return format!("{marker}{rest}");
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> PathContext {
        let home = PathBuf::from(r"C:\Users\João");
        PathContext {
            app_data: home.join(r"AppData\Roaming"),
            local_app_data: home.join(r"AppData\Local"),
            local_app_data_low: home.join(r"AppData\LocalLow"),
            documents: home.join("Documents"),
            public: PathBuf::from(r"C:\Users\Public"),
            program_data: PathBuf::from(r"C:\ProgramData"),
            os_user_name: "João".into(),
            store_user_id: None,
            home,
        }
    }

    #[test]
    fn expande_appdata() {
        let r = expand("<winAppData>/EldenRing", &ctx(), &GameVars::default());
        assert_eq!(
            r.as_deref(),
            Some("C:/Users/João/AppData/Roaming/EldenRing")
        );
    }

    #[test]
    fn documentos_redirecionados() {
        let mut c = ctx();
        c.documents = PathBuf::from(r"C:\Users\João\OneDrive\Documentos");
        let r = expand("<winDocuments>/My Games/X", &c, &GameVars::default()).unwrap();
        assert!(r.starts_with("C:/Users/João/OneDrive/Documentos"), "{r}");
    }

    #[test]
    fn sem_store_user_id_retorna_none() {
        let vars = GameVars {
            base: Some(PathBuf::from(r"D:\Jogos\X")),
            ..Default::default()
        };
        assert_eq!(expand("<base>/<storeUserId>/save", &ctx(), &vars), None);
    }

    #[test]
    fn expande_base_e_game() {
        let vars = GameVars {
            base: Some(PathBuf::from(r"D:\Steam\steamapps\common\Celeste")),
            root: Some(PathBuf::from(r"C:\Program Files (x86)\Steam")),
            game: Some("Celeste".into()),
            store_game_id: Some("504230".into()),
        };
        let r = expand("<base>/<game>/<storeGameId>", &ctx(), &vars);
        assert_eq!(
            r.as_deref(),
            Some("D:/Steam/steamapps/common/Celeste/Celeste/504230")
        );
    }

    #[test]
    fn root_e_a_raiz_da_loja() {
        let mut c = ctx();
        c.store_user_id = Some("64734272".into());
        let vars = GameVars {
            root: Some(PathBuf::from(r"C:\Program Files (x86)\Steam")),
            ..Default::default()
        };
        let r = expand("<root>/userdata/<storeUserId>/1086940/remote", &c, &vars);
        assert_eq!(
            r.as_deref(),
            Some("C:/Program Files (x86)/Steam/userdata/64734272/1086940/remote")
        );
        assert_eq!(expand("<root>/x", &c, &GameVars::default()), None);
    }

    #[test]
    fn expande_id64_a_partir_do_id3() {
        let mut c = ctx();
        c.store_user_id = Some("64734272".into());
        let r = expand(
            "<winAppData>/EldenRing/<storeUserId64>",
            &c,
            &GameVars::default(),
        );
        assert_eq!(
            r.as_deref(),
            Some("C:/Users/João/AppData/Roaming/EldenRing/76561198025000000")
        );
    }

    #[test]
    fn contract_com_base_e_root() {
        let vars = GameVars {
            base: Some(PathBuf::from(r"D:\SteamLibrary\steamapps\common\Jogo")),
            root: Some(PathBuf::from(r"C:\Steam")),
            ..Default::default()
        };
        let p = Path::new(r"D:\SteamLibrary\steamapps\common\Jogo\Saves\a.sav");
        assert_eq!(contract_with(p, &ctx(), &vars), "<base>/Saves/a.sav");
        let u = Path::new(r"C:\Steam\userdata\1\2\remote\s.dat");
        assert_eq!(
            contract_with(u, &ctx(), &vars),
            "<root>/userdata/1/2/remote/s.dat"
        );
    }

    #[test]
    fn contract_prefere_mais_longo() {
        let p = Path::new(r"C:\Users\João\AppData\Local\X\s.sav");
        assert_eq!(contract(p, &ctx()), "<winLocalAppData>/X/s.sav");
    }

    #[test]
    fn contract_ignora_maiusculas() {
        let p = Path::new(r"c:\users\joão\appdata\roaming\Jogo\a.sav");
        assert_eq!(contract(p, &ctx()), "<winAppData>/Jogo/a.sav");
    }

    #[test]
    fn ida_e_volta() {
        let c = ctx();
        for p in [
            r"C:\Users\João\AppData\Roaming\EldenRing\1\ER0000.sl2",
            r"C:\Users\João\AppData\LocalLow\Team Cherry\Hollow Knight\user1.dat",
            r"C:\Users\João\Documents\My Games\Skyrim\Saves\a.ess",
            r"C:\Users\Public\Documents\Steam\save.dat",
            r"C:\Users\João\Saved Games\Jogo\slot.sav",
        ] {
            let t = contract(Path::new(p), &c);
            assert!(t.starts_with('<'), "{t}");
            let back = expand(&t, &c, &GameVars::default()).unwrap();
            assert_eq!(back, p.replace('\\', "/"));
        }
    }
}
