//! Varredura do PC: cruza o manifesto, as lojas, os emuladores e a heurística.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use globset::GlobBuilder;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use crate::emulators::EMULATORS;
use crate::heuristics;
use crate::manifest::{CloudFlags, GameEntry, Manifest, Store};
use crate::paths::{GameVars, PathContext, expand, norm};
use crate::stores::InstalledGame;

/// Acima disso o jogo vai para "Grandes demais — confirmar".
pub const LIMITE_BYTES_POR_JOGO: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Manifesto,
    Emulador,
    /// Mostrado como "Possível save — confirmar".
    Heuristica,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetectedGame {
    pub id: String,
    pub name: String,
    pub store: Store,
    pub save_roots: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
    pub total_bytes: u64,
    pub cloud: CloudFlags,
    pub source: Source,
    pub exe_hint: Option<PathBuf>,
    /// Pasta de instalação (`<base>`), quando o jogo veio de uma loja.
    pub base: Option<PathBuf>,
    /// Raiz da loja (`<root>`).
    pub root: Option<PathBuf>,
    pub store_game_id: Option<String>,
    /// Padrões (já expandidos) que acharam os arquivos: refeitos antes de cada backup
    /// automático, porque jogos criam saves novos e apagam os antigos.
    pub patterns: Vec<String>,
    /// AppID da Steam (capa do jogo), do manifesto ou da loja.
    pub steam_app_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualGame {
    pub name: String,
    pub save_dir: PathBuf,
    pub exe: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// Pastas de emuladores portáteis (onde o executável foi achado).
    pub extra_roots: Vec<PathBuf>,
    pub manual: Vec<ManualGame>,
}

#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub games: Vec<DetectedGame>,
    pub over_limit: Vec<DetectedGame>,
}

/// Identificador estável: minúsculas, sem acentos, palavras separadas por hífen.
pub fn game_id(name: &str) -> String {
    let mut out = String::new();
    for c in name.nfd().filter(|c| c.is_ascii()) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '\'' {
            continue;
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        // Nomes sem letras latinas (cirílico, japonês, símbolos): id pelo hash do nome.
        format!("jogo-{}", short_hash(name))
    } else {
        slug
    }
}

fn short_hash(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex()[..8].to_string()
}

/// Garante ids únicos: repetidos ganham o hash do nome original.
fn dedupe_ids(games: &mut [DetectedGame]) {
    let mut count: HashMap<String, usize> = HashMap::new();
    for g in games.iter() {
        *count.entry(g.id.clone()).or_default() += 1;
    }
    for g in games.iter_mut() {
        if count[&g.id] > 1 {
            g.id = format!("{}-{}", g.id, short_hash(&g.name));
        }
    }
}

/// Guarda o conteúdo de cada pasta lida, para checar milhares de caminhos do
/// manifesto sem ir ao disco de novo. Ignora maiúsculas (Windows).
#[derive(Default)]
pub(crate) struct DirCache {
    /// pasta (minúsculas) → nome em minúsculas → nome original
    listings: HashMap<String, Option<HashMap<String, String>>>,
}

impl DirCache {
    fn listing(&mut self, dir: &Path) -> Option<&HashMap<String, String>> {
        let key = norm(dir).to_lowercase();
        self.listings
            .entry(key)
            .or_insert_with(|| {
                std::fs::read_dir(dir).ok().map(|rd| {
                    rd.flatten()
                        .map(|e| {
                            let n = e.file_name().to_string_lossy().into_owned();
                            (n.to_lowercase(), n)
                        })
                        .collect()
                })
            })
            .as_ref()
    }

    /// Filhos de `dir` (nomes originais).
    fn children(&mut self, dir: &Path) -> Vec<String> {
        self.listing(dir)
            .map(|l| l.values().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn exists(&mut self, p: &Path) -> bool {
        let (Some(parent), Some(name)) = (p.parent(), p.file_name()) else {
            return p.exists();
        };
        if parent.as_os_str().is_empty() || parent.parent().is_none() {
            return p.exists();
        }
        if !self.exists(parent) {
            return false;
        }
        let name = name.to_string_lossy().to_lowercase();
        if self.listing(parent).is_some_and(|l| l.contains_key(&name)) {
            return true;
        }
        // A listagem traz só nomes longos; um nome curto 8.3 (ex. RUNNER~1, que aparece
        // em %TEMP% e em perfis com nome comprido) só é confirmado indo ao disco.
        name.contains('~') && p.exists()
    }
}

/// Só `*` e `?` são curingas. Colchetes aparecem literais em nomes de pasta do manifesto.
fn has_glob(s: &str) -> bool {
    s.contains(['*', '?'])
}

/// Escapa colchetes para o globset tratá-los como texto.
fn escape_brackets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '[' => out.push_str("[[]"),
            ']' => out.push_str("[]]"),
            _ => out.push(c),
        }
    }
    out
}

fn matcher(glob: &str) -> Option<globset::GlobMatcher> {
    GlobBuilder::new(&escape_brackets(glob))
        .case_insensitive(true)
        .literal_separator(true)
        .build()
        .ok()
        .map(|g| g.compile_matcher())
}

fn files_under(dir: &Path, out: &mut BTreeSet<PathBuf>) {
    for e in WalkDir::new(dir).into_iter().flatten() {
        if e.file_type().is_file() {
            out.insert(e.into_path());
        }
    }
}

/// Resolve um padrão já expandido, nível por nível, usando o cache de pastas.
/// Adiciona os arquivos encontrados e a pasta-raiz do save.
fn collect(
    cache: &mut DirCache,
    pattern: &str,
    files: &mut BTreeSet<PathBuf>,
    roots: &mut BTreeSet<PathBuf>,
) {
    let parts: Vec<&str> = pattern.split('/').filter(|p| !p.is_empty()).collect();
    let Some((first, rest)) = parts.split_first() else {
        return;
    };
    let first_glob = parts.iter().position(|p| has_glob(p));
    let mut current = vec![PathBuf::from(format!("{first}/"))];
    for comp in rest {
        let mut next = Vec::new();
        if *comp == "**" {
            // Raro no manifesto: casa o resto do padrão andando nas subpastas.
            let Some(m) = matcher(pattern) else { return };
            for dir in &current {
                for e in WalkDir::new(dir).min_depth(1).into_iter().flatten() {
                    if m.is_match(norm(e.path())) {
                        next.push(e.into_path());
                    }
                }
            }
            current = next;
            break;
        }
        if has_glob(comp) {
            let Some(m) = matcher(comp) else { return };
            for dir in &current {
                for child in cache.children(dir) {
                    if m.is_match(&child) {
                        next.push(dir.join(child));
                    }
                }
            }
        } else {
            for dir in &current {
                let p = dir.join(comp);
                if cache.exists(&p) {
                    next.push(p);
                }
            }
        }
        if next.is_empty() {
            return;
        }
        current = next;
    }
    let before = files.len();
    for p in &current {
        if p.is_file() {
            files.insert(p.clone());
        } else if p.is_dir() {
            files_under(p, files);
        }
    }
    if files.len() == before {
        return;
    }
    match first_glob {
        Some(idx) => {
            roots.insert(PathBuf::from(parts[..idx].join("/")));
        }
        None => {
            for p in current {
                let root = if p.is_file() {
                    p.parent().map(Path::to_path_buf)
                } else {
                    Some(p)
                };
                roots.extend(root);
            }
        }
    }
}

fn build_game(
    id: String,
    name: String,
    store: Store,
    source: Source,
    cloud: CloudFlags,
    files: BTreeSet<PathBuf>,
    roots: BTreeSet<PathBuf>,
) -> Option<DetectedGame> {
    if files.is_empty() {
        return None;
    }
    let total_bytes = files
        .iter()
        .filter_map(|f| f.metadata().ok())
        .map(|m| m.len())
        .sum();
    Some(DetectedGame {
        id,
        name,
        store,
        save_roots: roots.into_iter().collect(),
        files: files.into_iter().collect(),
        total_bytes,
        cloud,
        source,
        exe_hint: None,
        base: None,
        root: None,
        store_game_id: None,
        patterns: Vec::new(),
        steam_app_id: None,
    })
}

fn name_key(s: &str) -> String {
    game_id(s).replace('-', "")
}

struct InstalledIndex<'a> {
    by_steam: HashMap<String, &'a InstalledGame>,
    by_gog: HashMap<String, &'a InstalledGame>,
    by_name: HashMap<String, &'a InstalledGame>,
    by_dir: HashMap<String, &'a InstalledGame>,
}

impl<'a> InstalledIndex<'a> {
    fn new(installed: &'a [InstalledGame]) -> Self {
        let mut idx = InstalledIndex {
            by_steam: HashMap::new(),
            by_gog: HashMap::new(),
            by_name: HashMap::new(),
            by_dir: HashMap::new(),
        };
        for g in installed {
            if let Some(id) = &g.store_game_id {
                match g.store {
                    Store::Steam => drop(idx.by_steam.insert(id.clone(), g)),
                    Store::Gog => drop(idx.by_gog.insert(id.clone(), g)),
                    _ => {}
                }
            }
            idx.by_name.insert(name_key(&g.name), g);
            if let Some(dir) = g.install_dir.file_name() {
                idx.by_dir.insert(dir.to_string_lossy().to_lowercase(), g);
            }
        }
        idx
    }

    fn find(&self, name: &str, e: &GameEntry) -> Option<&'a InstalledGame> {
        e.steam_id
            .and_then(|id| self.by_steam.get(&id.to_string()))
            .or_else(|| e.gog_id.and_then(|id| self.by_gog.get(&id.to_string())))
            .or_else(|| self.by_name.get(&name_key(name)))
            .or_else(|| {
                e.install_dir
                    .iter()
                    .find_map(|d| self.by_dir.get(&d.to_lowercase()))
            })
            .copied()
    }
}

/// Refaz a lista de arquivos de um jogo a partir dos padrões que o acharam
/// (saves criados ou apagados depois da varredura). Sem padrões, devolve igual.
pub fn refresh_files(g: &DetectedGame) -> DetectedGame {
    if g.patterns.is_empty() {
        return g.clone();
    }
    let mut cache = DirCache::default();
    let mut files = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for p in &g.patterns {
        collect(&mut cache, p, &mut files, &mut roots);
    }
    let mut n = g.clone();
    n.total_bytes = files
        .iter()
        .filter_map(|f| f.metadata().ok())
        .map(|m| m.len())
        .sum();
    n.files = files.into_iter().collect();
    if !roots.is_empty() {
        n.save_roots = roots.into_iter().collect();
    }
    n
}

/// Executáveis de emuladores conhecidos até 3 níveis abaixo das pastas dadas.
pub fn find_emulator_roots(search_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let exes: Vec<String> = EMULATORS
        .iter()
        .flat_map(|e| e.exes.iter().map(|x| x.to_lowercase()))
        .collect();
    let mut found = BTreeSet::new();
    for dir in search_dirs {
        for e in WalkDir::new(dir).max_depth(3).into_iter().flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if e.file_type().is_file()
                && exes.contains(&name)
                && let Some(parent) = e.path().parent()
            {
                found.insert(parent.to_path_buf());
            }
        }
    }
    found.into_iter().collect()
}

fn root_has_exe(root: &Path, exes: &[&str]) -> bool {
    exes.iter().any(|x| root.join(x).is_file())
}

pub fn scan(
    ctx: &PathContext,
    manifest: &Manifest,
    installed: &[InstalledGame],
    opts: &ScanOptions,
) -> ScanResult {
    scan_with_limit(ctx, manifest, installed, opts, LIMITE_BYTES_POR_JOGO)
}

pub(crate) fn scan_with_limit(
    ctx: &PathContext,
    manifest: &Manifest,
    installed: &[InstalledGame],
    opts: &ScanOptions,
    limit: u64,
) -> ScanResult {
    let mut games: Vec<DetectedGame> = Vec::new();
    let index = InstalledIndex::new(installed);
    let mut matched_installed: BTreeSet<PathBuf> = BTreeSet::new();
    let mut cache = DirCache::default();

    // 1. Manifesto. `<storeUserId>` desconhecido vira `*`, como no Ludusavi.
    for (name, entry) in &manifest.games {
        let inst = index.find(name, entry);
        let store = inst.map(|i| i.store);
        let mut c = ctx.clone();
        c.store_user_id = Some("*".into());
        let vars = GameVars {
            base: inst.map(|i| i.install_dir.clone()),
            root: inst.and_then(|i| i.store_root.clone()),
            game: Some(name.clone()),
            store_game_id: inst.and_then(|i| i.store_game_id.clone()),
        };
        let mut files = BTreeSet::new();
        let mut roots = BTreeSet::new();
        let mut pats = Vec::new();
        for pattern in entry.save_paths_for(store) {
            if let Some(p) = expand(pattern, &c, &vars) {
                collect(&mut cache, &p, &mut files, &mut roots);
                pats.push(p);
            }
        }
        if let Some(mut g) = build_game(
            game_id(name),
            name.clone(),
            store.unwrap_or(Store::SemLoja),
            Source::Manifesto,
            entry.cloud,
            files,
            roots,
        ) {
            g.patterns = pats;
            g.steam_app_id = entry.steam_id.or_else(|| {
                inst.filter(|i| i.store == Store::Steam)
                    .and_then(|i| i.store_game_id.as_deref()?.parse().ok())
            });
            if let Some(i) = inst {
                matched_installed.insert(i.install_dir.clone());
                g.base = Some(i.install_dir.clone());
                g.root = i.store_root.clone();
                g.store_game_id = i.store_game_id.clone();
            }
            games.push(g);
        }
    }

    // 2. Pacotes Xbox sem entrada no manifesto: backup bruto da pasta wgs.
    for i in installed.iter().filter(|i| i.store == Store::Xbox) {
        if matched_installed.contains(&i.install_dir) {
            continue;
        }
        let mut files = BTreeSet::new();
        files_under(&i.install_dir, &mut files);
        let roots = BTreeSet::from([i.install_dir.clone()]);
        let cloud = CloudFlags::default();
        if let Some(g) = build_game(
            game_id(&i.name),
            i.name.clone(),
            Store::Xbox,
            Source::Heuristica,
            cloud,
            files,
            roots,
        ) {
            let mut g = g;
            g.patterns = vec![norm(&i.install_dir)];
            games.push(g);
        }
    }

    // 3. Emuladores (instalados no perfil ou portáteis em `extra_roots`).
    for emu in EMULATORS {
        let mut files = BTreeSet::new();
        let mut roots = BTreeSet::new();
        let mut pats = Vec::new();
        let bases: Vec<&PathBuf> = opts
            .extra_roots
            .iter()
            .filter(|r| root_has_exe(r, emu.exes))
            .collect();
        for pattern in emu.patterns {
            if pattern.contains("<base>") {
                for base in &bases {
                    let vars = GameVars {
                        base: Some((*base).clone()),
                        ..Default::default()
                    };
                    if let Some(p) = expand(pattern, ctx, &vars) {
                        collect(&mut cache, &p, &mut files, &mut roots);
                        pats.push(p);
                    }
                }
            } else if let Some(p) = expand(pattern, ctx, &GameVars::default()) {
                collect(&mut cache, &p, &mut files, &mut roots);
                pats.push(p);
            }
        }
        if let Some(mut g) = build_game(
            emu.id.to_string(),
            emu.name.to_string(),
            Store::Emulador,
            Source::Emulador,
            CloudFlags::default(),
            files,
            roots,
        ) {
            g.exe_hint = bases.first().map(|b| b.join(emu.exes[0]));
            g.patterns = pats;
            games.push(g);
        }
    }

    // 4. Jogos adicionados à mão.
    for m in &opts.manual {
        let mut files = BTreeSet::new();
        files_under(&m.save_dir, &mut files);
        let roots = BTreeSet::from([m.save_dir.clone()]);
        if let Some(mut g) = build_game(
            game_id(&m.name),
            m.name.clone(),
            Store::SemLoja,
            Source::Manual,
            CloudFlags::default(),
            files,
            roots,
        ) {
            g.exe_hint = m.exe.clone();
            g.patterns = vec![norm(&m.save_dir)];
            games.push(g);
        }
    }

    // 5. Heurística: pastas com cara de save que nada acima cobriu.
    let covered: Vec<String> = games
        .iter()
        .flat_map(|g| g.save_roots.iter().map(|r| norm(r).to_lowercase()))
        .collect();
    for cand in heuristics::candidates(ctx) {
        let d = norm(&cand.dir).to_lowercase();
        let overlaps = covered
            .iter()
            .any(|r| r == &d || r.starts_with(&format!("{d}/")) || d.starts_with(&format!("{r}/")));
        if overlaps {
            continue;
        }
        let mut files = BTreeSet::new();
        files_under(&cand.dir, &mut files);
        let roots = BTreeSet::from([cand.dir.clone()]);
        if let Some(g) = build_game(
            game_id(&cand.name),
            cand.name,
            Store::SemLoja,
            Source::Heuristica,
            CloudFlags::default(),
            files,
            roots,
        ) {
            let mut g = g;
            g.patterns = vec![norm(&cand.dir)];
            games.push(g);
        }
    }

    dedupe_ids(&mut games);
    games.sort_by_key(|g| g.name.to_lowercase());
    let (games, over_limit) = games.into_iter().partition(|g| g.total_bytes <= limit);
    ScanResult { games, over_limit }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Store, parse};
    use crate::paths::PathContext;
    use crate::stores::InstalledGame;
    use std::fs;
    use std::path::Path;

    fn ctx(root: &Path) -> PathContext {
        let home = root.join("Users/Joao");
        PathContext {
            app_data: home.join("AppData/Roaming"),
            local_app_data: home.join("AppData/Local"),
            local_app_data_low: home.join("AppData/LocalLow"),
            documents: home.join("Documents"),
            public: root.join("Users/Public"),
            program_data: root.join("ProgramData"),
            os_user_name: "Joao".into(),
            store_user_id: None,
            home,
        }
    }

    fn touch(p: &Path, bytes: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b'x'; bytes]).unwrap();
    }

    fn manifest() -> crate::manifest::Manifest {
        parse(include_str!("../tests/fixtures/manifest-mini.yaml")).unwrap()
    }

    fn elden_installed(root: &Path) -> InstalledGame {
        InstalledGame {
            name: "ELDEN RING".into(),
            store: Store::Steam,
            install_dir: root.join("Steam/steamapps/common/ELDEN RING"),
            store_game_id: Some("1245620".into()),
            store_user_id: Some("64734272".into()),
            store_root: Some(root.join("Steam")),
        }
    }

    #[test]
    fn acha_jogo_do_manifesto() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(
            &c.app_data.join("EldenRing/76561198025000000/ER0000.sl2"),
            10,
        );
        let r = scan(
            &c,
            &manifest(),
            &[elden_installed(t.path())],
            &ScanOptions::default(),
        );
        let er: Vec<_> = r.games.iter().filter(|g| g.name == "Elden Ring").collect();
        assert_eq!(er.len(), 1, "{:?}", r.games);
        assert_eq!(er[0].source, Source::Manifesto);
        assert_eq!(er[0].store, Store::Steam);
        assert_eq!(er[0].files.len(), 1);
        assert_eq!(er[0].total_bytes, 10);
        assert!(er[0].cloud.steam);
    }

    #[test]
    fn userdata_da_steam_pelo_root() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        let steam = t.path().join("Steam");
        touch(&steam.join("userdata/64734272/1086940/remote/save.lsv"), 4);
        let inst = InstalledGame {
            name: "Jogo U".into(),
            store: Store::Steam,
            install_dir: steam.join("steamapps/common/JogoU"),
            store_game_id: Some("1086940".into()),
            store_user_id: Some("64734272".into()),
            store_root: Some(steam.clone()),
        };
        let m = parse("Jogo U:\n  files:\n    \"<root>/userdata/<storeUserId>/1086940/remote\":\n      tags: [save]\n  steam:\n    id: 1086940\n").unwrap();
        let r = scan(&c, &m, &[inst], &ScanOptions::default());
        let g = r.games.iter().find(|g| g.name == "Jogo U").expect("Jogo U");
        assert_eq!(g.files.len(), 1);
        assert_eq!(g.root.as_deref(), Some(steam.as_path()));
        assert_eq!(g.store_game_id.as_deref(), Some("1086940"));
    }

    #[test]
    fn refresh_pega_save_novo_e_esquece_apagado() {
        // Jogos que criam saves numerados e apagam autosaves antigos.
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.home.join("Saved Games/Hades II/Profile1.sav"), 5);
        touch(&c.home.join("Saved Games/Hades II/Profile2.sav"), 5);
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        let g = r
            .games
            .iter()
            .find(|g| g.name == "Hades II")
            .unwrap()
            .clone();
        assert_eq!(g.files.len(), 2);
        fs::remove_file(c.home.join("Saved Games/Hades II/Profile1.sav")).unwrap();
        touch(&c.home.join("Saved Games/Hades II/Profile3.sav"), 7);
        let novo = refresh_files(&g);
        let nomes: Vec<_> = novo
            .files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(nomes, ["Profile2.sav", "Profile3.sav"]);
        assert_eq!(novo.total_bytes, 12);
    }

    #[test]
    fn refresh_de_jogo_manual_relista_a_pasta() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        let dir = t.path().join("Jogos/Meu/saves");
        touch(&dir.join("a.sav"), 1);
        let opts = ScanOptions {
            manual: vec![ManualGame {
                name: "Meu".into(),
                save_dir: dir.clone(),
                exe: None,
            }],
            ..Default::default()
        };
        let g = scan(&c, &manifest(), &[], &opts)
            .games
            .into_iter()
            .find(|g| g.id == "meu")
            .unwrap();
        touch(&dir.join("sub/b.sav"), 1);
        assert_eq!(refresh_files(&g).files.len(), 2);
    }

    #[test]
    fn jogo_do_manifesto_sem_loja_tambem_aparece() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.home.join("Saved Games/Hades II/Profile1.sav"), 5);
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        let h = r
            .games
            .iter()
            .find(|g| g.name == "Hades II")
            .expect("Hades II");
        assert_eq!(h.store, Store::SemLoja);
        assert_eq!(h.files.len(), 1);
    }

    #[test]
    fn appid_vem_do_manifesto() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.home.join("Saved Games/Hades II/Profile1.sav"), 5);
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        let h = r.games.iter().find(|g| g.name == "Hades II").unwrap();
        assert_eq!(h.steam_app_id, Some(1145350));
    }

    #[test]
    fn jogo_sem_pasta_nao_aparece() {
        let t = tempfile::tempdir().unwrap();
        let r = scan(&ctx(t.path()), &manifest(), &[], &ScanOptions::default());
        assert!(!r.games.iter().any(|g| g.name == "Celeste"));
    }

    #[test]
    fn emulador_pcsx2() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.documents.join("PCSX2/memcards/Mcd001.ps2"), 8);
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        let g = r.games.iter().find(|g| g.id == "pcsx2").expect("pcsx2");
        assert_eq!(g.store, Store::Emulador);
        assert_eq!(g.source, Source::Emulador);
        assert_eq!(g.files.len(), 1);
    }

    #[test]
    fn emulador_portatil_por_extra_root() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        let mgba = t.path().join("Emuladores/mGBA");
        touch(&mgba.join("mGBA.exe"), 1);
        touch(&mgba.join("Pokemon Emerald.sav"), 128);
        let found = find_emulator_roots(&[t.path().join("Emuladores")]);
        assert_eq!(found, vec![mgba.clone()]);
        let opts = ScanOptions {
            extra_roots: found,
            ..Default::default()
        };
        let r = scan(&c, &manifest(), &[], &opts);
        let g = r.games.iter().find(|g| g.id == "mgba").expect("mgba");
        assert_eq!(g.files, vec![mgba.join("Pokemon Emerald.sav")]);
    }

    #[test]
    fn heuristica_pasta_desconhecida() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.documents.join("My Games/JogoIndie/slot1.sav"), 3);
        touch(&c.app_data.join("Microsoft/Algo/save.dat"), 3);
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        let g = r
            .games
            .iter()
            .find(|g| g.name == "JogoIndie")
            .expect("JogoIndie");
        assert_eq!(g.source, Source::Heuristica);
        assert!(
            !r.games
                .iter()
                .any(|g| g.name == "Microsoft" || g.name == "Algo")
        );
    }

    #[test]
    fn heuristica_ignora_cache_de_webview() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(
            &c.local_app_data_low
                .join("Empresa/EBWebView/Default/save.dat"),
            3,
        );
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        assert!(
            !r.games.iter().any(|g| g.name == "EBWebView"),
            "{:?}",
            r.games
        );
    }

    #[test]
    fn regra_de_outra_loja_vale_sem_loja() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        let m = parse(
            "Jogo X:
  files:
    \"<home>/AppData/LocalLow/Studio/Jogo X/<storeUserId>\":
      tags: [save]
      when:
        - os: windows
          store: steam
",
        )
        .unwrap();
        touch(&c.local_app_data_low.join("Studio/Jogo X/123/slot.sav"), 3);
        let r = scan(&c, &m, &[], &ScanOptions::default());
        let g = r.games.iter().find(|g| g.name == "Jogo X").expect("Jogo X");
        assert_eq!(g.source, Source::Manifesto);
    }

    #[test]
    fn colchete_e_literal() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.app_data.join("[Estudio] Jogo/Saves/a.sav"), 2);
        touch(&c.app_data.join("E Jogo/Saves/b.sav"), 2);
        let m = parse(
            "Jogo C:
  files:
    \"<winAppData>/[Estudio] Jogo/Saves\":
      tags: [save]
",
        )
        .unwrap();
        let r = scan(&c, &m, &[], &ScanOptions::default());
        let g = r.games.iter().find(|g| g.name == "Jogo C").expect("Jogo C");
        assert_eq!(g.files.len(), 1);
        assert!(g.files[0].ends_with("a.sav"));
    }

    #[test]
    fn curinga_no_meio_do_caminho() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(
            &c.local_app_data
                .join("Packages/Estudio.Jogo_abc123/SystemAppData/wgs/x"),
            2,
        );
        touch(
            &c.local_app_data
                .join("Packages/Outro_zzz/SystemAppData/wgs/y"),
            2,
        );
        let m = parse(
            "Jogo W:
  files:
    \"<winLocalAppData>/Packages/Estudio.Jogo_*/SystemAppData/wgs\":
      tags: [save]
",
        )
        .unwrap();
        let r = scan(&c, &m, &[], &ScanOptions::default());
        let g = r.games.iter().find(|g| g.name == "Jogo W").expect("Jogo W");
        assert_eq!(g.files.len(), 1);
        assert!(g.files[0].ends_with("x"));
    }

    /// Nome curto 8.3 do Windows (ex. `RUNNER~1`) para um caminho que existe.
    #[cfg(windows)]
    fn short_path(p: &std::path::Path) -> std::path::PathBuf {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let wide: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
        let mut buf = vec![0u16; 1024];
        // SAFETY: `wide` termina em zero e `buf` tem o tamanho informado.
        let n = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(
                wide.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        } as usize;
        std::path::PathBuf::from(std::ffi::OsString::from_wide(&buf[..n]))
    }

    #[cfg(windows)]
    #[test]
    fn dir_cache_aceita_nome_curto_8_3() {
        let t = tempfile::tempdir().unwrap();
        let longa = t.path().join("Pasta Com Nome Bem Comprido");
        touch(&longa.join("slot.sav"), 1);
        let curta = short_path(&longa);
        if curta.file_name() == longa.file_name() {
            eprintln!("nomes 8.3 desligados neste volume; teste sem efeito");
            return;
        }
        let mut cache = DirCache::default();
        assert!(cache.exists(&curta.join("slot.sav")), "{curta:?}");
        assert!(!cache.exists(&curta.join("outro.sav")));
    }

    #[test]
    fn dir_cache_ignora_maiusculas() {
        let t = tempfile::tempdir().unwrap();
        touch(&t.path().join("Pasta/Arquivo.SAV"), 1);
        let mut cache = DirCache::default();
        assert!(cache.exists(&t.path().join("pasta/arquivo.sav")));
        assert!(!cache.exists(&t.path().join("pasta/outro.sav")));
        assert!(!cache.exists(&t.path().join("nada/arquivo.sav")));
    }

    #[test]
    fn heuristica_nao_repete_jogo_conhecido() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(
            &c.local_app_data_low
                .join("Team Cherry/Hollow Knight/user1.dat"),
            4,
        );
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        let hk: Vec<_> = r
            .games
            .iter()
            .filter(|g| g.name.contains("Hollow"))
            .collect();
        assert_eq!(hk.len(), 1, "{hk:?}");
        assert_eq!(hk[0].source, Source::Manifesto);
    }

    #[test]
    fn jogo_manual() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        let dir = t.path().join("Jogos/MeuJogo/saves");
        touch(&dir.join("a.sav"), 1);
        touch(&dir.join("sub/b.sav"), 1);
        let opts = ScanOptions {
            manual: vec![ManualGame {
                name: "Meu Jogo".into(),
                save_dir: dir.clone(),
                exe: None,
            }],
            ..Default::default()
        };
        let r = scan(&c, &manifest(), &[], &opts);
        let g = r.games.iter().find(|g| g.id == "meu-jogo").expect("manual");
        assert_eq!(g.source, Source::Manual);
        assert_eq!(g.files.len(), 2);
        assert_eq!(g.save_roots, vec![dir]);
    }

    #[test]
    fn acima_do_limite() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.home.join("Saved Games/Hades II/Profile1.sav"), 2000);
        let r = scan_with_limit(&c, &manifest(), &[], &ScanOptions::default(), 1000);
        assert!(!r.games.iter().any(|g| g.name == "Hades II"));
        assert!(r.over_limit.iter().any(|g| g.name == "Hades II"));
    }

    #[test]
    fn game_id_de_nome_sem_letras_latinas_nao_fica_vazio() {
        let a = game_id("Звёздный путь");
        let b = game_id("星のカービィ");
        assert!(a.starts_with("jogo-") && a.len() > 5, "{a}");
        assert!(b.starts_with("jogo-"), "{b}");
        assert_ne!(a, b);
        assert_eq!(game_id("***"), game_id("***"));
    }

    #[test]
    fn ids_repetidos_sao_desambiguados() {
        let t = tempfile::tempdir().unwrap();
        let c = ctx(t.path());
        touch(&c.app_data.join("KlausA/s.sav"), 1);
        touch(&c.app_data.join("KlausB/s.sav"), 1);
        let m = parse(
            "Klaus:
  files:
    \"<winAppData>/KlausA\":
      tags: [save]
-KLAUS-:
  files:
    \"<winAppData>/KlausB\":
      tags: [save]
",
        )
        .unwrap();
        let r = scan(&c, &m, &[], &ScanOptions::default());
        let ids: Vec<_> = r
            .games
            .iter()
            .filter(|g| g.name.to_lowercase().contains("klaus"))
            .map(|g| g.id.clone())
            .collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn game_id_sem_acento() {
        assert_eq!(game_id("Pokémon Emerald"), "pokemon-emerald");
        assert_eq!(game_id("Baldur's Gate 3"), "baldurs-gate-3");
        assert_eq!(
            game_id("  ELDEN RING: Nightreign "),
            "elden-ring-nightreign"
        );
    }

    #[test]
    fn caminho_sem_ascii_funciona() {
        let t = tempfile::tempdir().unwrap();
        let mut c = ctx(t.path());
        c.home = t.path().join("Users/João");
        touch(&c.home.join("Saved Games/Hades II/Profile1.sav"), 1);
        let r = scan(&c, &manifest(), &[], &ScanOptions::default());
        assert!(r.games.iter().any(|g| g.name == "Hades II"));
    }
}

#[cfg(test)]
mod pc_real {
    /// `SAVEGUARD_MANIFEST=<arquivo> cargo test --release -- --ignored varre_pc_real --nocapture`
    #[test]
    #[ignore]
    fn varre_pc_real() {
        let path = std::env::var("SAVEGUARD_MANIFEST").expect("defina SAVEGUARD_MANIFEST");
        let m = crate::manifest::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        let ctx = crate::paths::PathContext::from_system();
        let t = std::time::Instant::now();
        let installed = crate::stores::detect_all();
        let roots: Vec<_> = ["C:\\", "D:\\"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        let emus = super::find_emulator_roots(&roots);
        eprintln!("busca de emuladores: {:?}", t.elapsed());
        let th = std::time::Instant::now();
        let n = crate::heuristics::candidates(&ctx).len();
        eprintln!("heurística: {n} candidatos em {:?}", th.elapsed());
        let opts = super::ScanOptions {
            extra_roots: emus.clone(),
            ..Default::default()
        };
        let r = super::scan(&ctx, &m, &installed, &opts);
        eprintln!("emuladores portáteis: {emus:?}");
        for g in r.games.iter().chain(&r.over_limit) {
            eprintln!(
                "{:?} | {:?} | {} | {} arquivos | {} KB",
                g.source,
                g.store,
                g.name,
                g.files.len(),
                g.total_bytes / 1024
            );
        }
        eprintln!(
            "jogos: {} (+{} grandes) em {:?}",
            r.games.len(),
            r.over_limit.len(),
            t.elapsed()
        );
    }
}
