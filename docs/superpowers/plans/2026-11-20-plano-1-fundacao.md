# SaveGuard — Plano 1: Fundação (varredura, prioridade, backup manual, restauração)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** App Tauri que varre o PC, lista os jogos com prioridade de risco, faz backup manual versionado para um destino escolhido e restaura num PC novo. É o mínimo para formatar sem perder saves.

**Architecture:** Motor em Rust numa crate pura `crates/core` (sem Tauri, 100% testável), exposto por comandos Tauri finos em `src-tauri`. Interface em Svelte 5 + TS + Tailwind, por enquanto só com o tema 1 (gamer verde-neon). Os planos 2 (backup contínuo, bandeja, pendrive) e 3 (5 temas, assistente, capas) vêm depois.

**Tech Stack:** Rust stable (edition 2021), Tauri 2, Svelte 5 + Vite + TypeScript + Tailwind 4, Vitest. Crates: `serde`, `serde_json`, `serde_yaml`, `zip`, `blake3`, `walkdir`, `globset`, `dirs`, `keyvalues-parser`, `winreg`, `chrono`, `thiserror`, `tempfile` (dev).

**Spec:** `docs/superpowers/specs/2026-11-20-saveguard-design.md`

## Global Constraints

- Somente Windows 10/11 64 bits. Código específico de Windows fica atrás de `#[cfg(windows)]`, para os testes de lógica rodarem em qualquer lugar.
- O app **só lê e copia** os saves originais. Nunca move nem apaga (anti-objetivo).
- Formato aberto: `<destino>\SaveGuard\biblioteca.json` + `<jogo-id>\<AAAA-MM-DD_HH-MM-SS>.zip` + `.json` irmão. Caminhos sempre com marcadores, nunca absolutos.
- Retenção padrão: **10** versões por jogo. Versões fixadas nunca são apagadas.
- Prioridade: **Alta** = nenhuma cópia fora do PC; **Média** = só nuvem de loja não confirmada ou backup do app com mais de **7 dias**; **Baixa** = nuvem confirmada ou backup com até 7 dias.
- Restaurar sempre guarda o save atual como versão antes de sobrescrever, e valida cada caminho do zip (zip slip).
- Interface em pt-BR. Nada de `innerHTML` com dados de jogo ou caminho; Tauri com CSP estrita e só os comandos listados expostos.
- Sem telemetria. A única rede do plano 1 é o download do manifesto.
- Commits em Conventional Commits, em português, terminando com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Trabalho em `feature/1-fundacao` a partir da `dev`; PR para `dev` com "Fecha #1" só no fim do plano 3. No plano 1 o PR usa "Parte de #1".

## Review Focus

1. **Usuário com acento ou Documentos redirecionados pro OneDrive** (`C:\Users\João\OneDrive\Documentos`): os marcadores precisam usar as pastas conhecidas do Windows (`dirs`), não `%USERPROFILE%\Documents`. Teste na Task 2.
2. **Arquivo travado ou sendo escrito durante o backup** (jogo aberto): não pode gerar versão pela metade. Erro claro, nada gravado. Teste na Task 7.
3. **Glob do manifesto que casa uma pasta enorme** (ex.: AppData inteiro, vários GB): limite de **2 GB** por jogo, acima disso pede confirmação e não trava. Teste na Task 5.
4. **Restaurar num PC com outro ID de usuário Steam ou outro nome de usuário:** `<storeUserId>` e `<home>` resolvidos para o PC novo. Teste na Task 9.
5. **Destino some no meio do backup** (pendrive arrancado): `biblioteca.json` nunca fica corrompido. Escrita atômica (arquivo temporário + rename) e zip só entra no índice depois de completo. Teste na Task 7.

---

### Task 1: Esqueleto do projeto e CI

**Files:**
- Create: `Cargo.toml` (workspace: `crates/core`, `src-tauri`), `crates/core/Cargo.toml`, `crates/core/src/lib.rs`
- Create: projeto Tauri 2 + Svelte-TS via `npm create tauri-app@latest` (template `svelte-ts`), depois mover `src-tauri` para o workspace
- Create: `.github/workflows/ci.yml`, `.github/dependabot.yml`
- Modify: `.gitignore` (já existe)

- [ ] **Step 1:** Criar a branch `feature/1-fundacao` a partir da `dev`.
- [ ] **Step 2:** Gerar o app Tauri (identificador `com.saveguard.app`, nome da janela "SaveGuard") e adicionar Tailwind 4 e Vitest.
- [ ] **Step 3:** Criar `crates/core` com `pub fn version() -> &'static str` e o teste `version_nao_vazia`.
- [ ] **Step 4:** Rodar `cargo test --workspace` e `npm run test -- --run`. Esperado: tudo PASS.
- [ ] **Step 5:** CI em `windows-latest`: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `npm ci && npm run check && npm run test -- --run`, `cargo audit`, `npm audit --audit-level=high`. Dependabot para `cargo`, `npm` e `github-actions`, semanal.
- [ ] **Step 6:** CSP em `src-tauri/tauri.conf.json`: `default-src 'self'; img-src 'self' data: https://cdn.cloudflare.steamstatic.com; style-src 'self' 'unsafe-inline'`.
- [ ] **Step 7:** Commit `build: cria esqueleto Tauri + Svelte e CI`.

### Task 2: Marcadores de caminho

**Files:** Create `crates/core/src/paths.rs`; testes no mesmo arquivo.

**Interfaces:**
- Produces:
  - `pub struct PathContext { pub home: PathBuf, pub app_data: PathBuf, pub local_app_data: PathBuf, pub local_app_data_low: PathBuf, pub documents: PathBuf, pub public: PathBuf, pub program_data: PathBuf, pub os_user_name: String, pub store_user_id: Option<String> }`
  - `impl PathContext { pub fn from_system() -> PathContext }` (usa `dirs`; `local_app_data_low` = `local_app_data/../LocalLow`)
  - `pub struct GameVars { pub base: Option<PathBuf>, pub game: Option<String>, pub store_game_id: Option<String> }`
  - `pub fn expand(template: &str, ctx: &PathContext, vars: &GameVars) -> Option<String>`: devolve padrão glob com `/`; `None` se o marcador exigido não estiver disponível.
  - `pub fn contract(path: &Path, ctx: &PathContext) -> String`: troca o prefixo mais longo que casar pelo marcador.
  - Em `lib.rs`: `pub enum CoreError` com `thiserror`, começando com `Io(#[from] std::io::Error)` e `Formato(String)`. As tasks seguintes acrescentam variantes.
- Marcadores suportados: `<home> <winAppData> <winLocalAppData> <winLocalAppDataLow> <winDocuments> <winPublic> <winProgramData> <osUserName> <storeUserId> <base> <game> <storeGameId> <root>` (`<root>` = pai de `<base>`).

- [ ] **Step 1: Testes**
  - `expande_appdata`: `expand("<winAppData>/EldenRing", ctx, vars)` == `"C:/Users/João/AppData/Roaming/EldenRing"`
  - `documentos_redirecionados`: ctx com `documents = C:\Users\João\OneDrive\Documentos`; `expand("<winDocuments>/My Games/X")` começa com `"C:/Users/João/OneDrive/Documentos"`
  - `sem_store_user_id_retorna_none`: `expand("<base>/<storeUserId>/save", ...)` com `store_user_id: None` == `None`
  - `contract_prefere_mais_longo`: `contract("C:\Users\João\AppData\Local\X\s.sav")` == `"<winLocalAppData>/X/s.sav"` (e não `<home>/AppData/...`)
  - `ida_e_volta`: `expand(contract(p))` == `p` normalizado, para 5 caminhos de exemplo
- [ ] **Step 2:** `cargo test -p saveguard-core paths`. Esperado: FAIL (não compila).
- [ ] **Step 3:** Implementar. A comparação de prefixo ignora maiúsculas/minúsculas (Windows).
- [ ] **Step 4:** Rodar de novo. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(motor): adiciona expansão e contração de marcadores de caminho`.

### Task 3: Manifesto do Ludusavi

**Files:** Create `crates/core/src/manifest.rs`, `crates/core/tests/fixtures/manifest-mini.yaml` (5 jogos reais copiados do manifesto: Elden Ring, Hollow Knight, Hades II, Baldur's Gate 3, Celeste); Create `src-tauri/resources/manifest.yaml.zst` (gerado por `scripts/atualizar-manifesto.ps1`).

**Interfaces:**
- Produces:
  - `pub struct Manifest { pub games: BTreeMap<String, GameEntry> }`
  - `pub struct GameEntry { pub files: BTreeMap<String, FileRule>, pub install_dir: Vec<String>, pub steam_id: Option<u32>, pub gog_id: Option<u64>, pub cloud: CloudFlags }`
  - `pub struct FileRule { pub tags: Vec<String>, pub when: Vec<When> }`, `pub struct When { pub os: Option<String>, pub store: Option<String> }`
  - `pub struct CloudFlags { pub steam: bool, pub epic: bool, pub gog: bool, pub origin: bool, pub uplay: bool }`
  - `pub fn parse(yaml: &str) -> Result<Manifest, CoreError>`
  - `impl GameEntry { pub fn save_paths_for(&self, store: Option<Store>) -> Vec<&str> }`: só regras com tag `save`, `os` ausente ou `windows`, `store` ausente ou igual
  - `pub enum Store { Steam, Epic, Gog, Xbox, Emulador, SemLoja }` (definido aqui, usado em tudo)
- URL do manifesto: `https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml`

- [ ] **Step 1: Testes**
  - `parse_mini`: 5 jogos; `games["Elden Ring"].steam_id == Some(1245620)`; `cloud.steam == true`
  - `filtra_linux`: regra com `when: [{os: linux}]` não aparece em `save_paths_for`
  - `filtra_por_loja`: regra com `store: gog` não aparece para `Some(Store::Steam)`
  - `ignora_campos_desconhecidos`: YAML com chave extra `registry:` faz o parse sem erro
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar com `serde_yaml` (`#[serde(default)]` nos campos opcionais).
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** `scripts/atualizar-manifesto.ps1` baixa a URL e grava `src-tauri/resources/manifest.yaml.zst`. Conferir a licença do manifesto e da PCGamingWiki e registrar o crédito no README e na tela "Sobre" (Task 10).
- [ ] **Step 6:** Commit `feat(motor): lê o manifesto de saves do Ludusavi`.

### Task 4: Jogos instalados por loja

**Files:** Create `crates/core/src/stores/{mod,steam,epic,gog}.rs`, fixtures em `crates/core/tests/fixtures/stores/` (`libraryfolders.vdf`, `appmanifest_1245620.acf`, `loginusers.vdf`, um `.item` da Epic).

**Interfaces:**
- Produces:
  - `pub struct InstalledGame { pub name: String, pub store: Store, pub install_dir: PathBuf, pub store_game_id: Option<String>, pub store_user_id: Option<String> }`
  - `pub fn steam_from_dir(steam_root: &Path) -> Vec<InstalledGame>` (lê `steamapps/libraryfolders.vdf`, cada `appmanifest_*.acf` e o `config/loginusers.vdf`; `store_user_id` = SteamID3 do usuário `MostRecent`, ou seja `steamid64 - 76561197960265728`)
  - `pub fn epic_from_dir(manifests_dir: &Path) -> Vec<InstalledGame>` (JSON `.item`: `DisplayName`, `InstallLocation`, `AppName`)
  - `#[cfg(windows)] pub fn gog_from_registry() -> Vec<InstalledGame>`
  - `pub fn xbox_from_packages(packages_dir: &Path) -> Vec<InstalledGame>`: cada `%LOCALAPPDATA%\Packages\<pkg>\SystemAppData\wgs` com conteúdo vira um jogo `Store::Xbox` (nome = parte do `<pkg>` antes do `_`, `install_dir` = a pasta `wgs`). A Task 5 adiciona a pasta `wgs` inteira como save (backup bruto), e a interface avisa que restaurar pode não funcionar.
  - `pub fn detect_all() -> Vec<InstalledGame>` (acha a raiz do Steam por `HKCU\Software\Valve\Steam\SteamPath`; Epic em `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests`)

- [ ] **Step 1: Testes**
  - `steam_le_bibliotecas`: fixture com 2 bibliotecas; acha `Elden Ring` com `install_dir` terminando em `steamapps/common/ELDEN RING` e `store_game_id == Some("1245620")`
  - `steam_user_id3`: `loginusers.vdf` com `76561198025000000` → `store_user_id == Some("64734272")`
  - `epic_le_item`: acha `Hades II` com `InstallLocation` da fixture
  - `pasta_inexistente_vazia`: `steam_from_dir("Z:/nada")` == `vec![]` (sem panic)
  - `xbox_wgs`: tempdir com `Microsoft.Forza_8wekyb3d8bbwe/SystemAppData/wgs/arquivo` → 1 jogo `Store::Xbox` com nome `Microsoft.Forza`; pacote sem `wgs` não aparece
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar (`keyvalues-parser` para VDF, `serde_json` para Epic, `winreg` para GOG/Steam).
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(motor): detecta jogos instalados na Steam, Epic e GOG`.

### Task 5: Varredura

**Files:** Create `crates/core/src/scan.rs`, `crates/core/src/emulators.rs`, `crates/core/src/heuristics.rs`.

**Interfaces:**
- Consumes: `PathContext`, `GameVars`, `expand` (Task 2); `Manifest`, `Store` (Task 3); `InstalledGame` (Task 4).
- Produces:
  - `pub struct DetectedGame { pub id: String, pub name: String, pub store: Store, pub save_roots: Vec<PathBuf>, pub files: Vec<PathBuf>, pub total_bytes: u64, pub cloud: CloudFlags, pub source: Source, pub exe_hint: Option<PathBuf> }`
  - `pub enum Source { Manifesto, Emulador, Heuristica, Manual }`. `Heuristica` aparece na interface como "Possível save — confirmar".
  - `pub const LIMITE_BYTES_POR_JOGO: u64 = 2 * 1024 * 1024 * 1024;`
  - `pub struct ScanOptions { pub extra_roots: Vec<PathBuf>, pub manual: Vec<ManualGame> }`, `pub struct ManualGame { pub name: String, pub save_dir: PathBuf, pub exe: Option<PathBuf> }`
  - `pub fn scan(ctx: &PathContext, manifest: &Manifest, installed: &[InstalledGame], opts: &ScanOptions) -> ScanResult`
  - `pub struct ScanResult { pub games: Vec<DetectedGame>, pub over_limit: Vec<DetectedGame> }`
  - `pub fn game_id(name: &str) -> String` (slug minúsculo com hífen, sem acentos: "Pokémon Emerald" → `pokemon-emerald`)
- Emuladores (`emulators.rs`): tabela fixa `(nome, &[padrões de save])` para RetroArch (`<winAppData>/RetroArch/saves`, `.../states`), PCSX2 (`<winDocuments>/PCSX2/memcards`), DuckStation (`<winDocuments>/DuckStation/memcards`), Dolphin (`<winDocuments>/Dolphin Emulator/GC`, `.../Wii`), PPSSPP (`<winDocuments>/PPSSPP/PSP/SAVEDATA`), RPCS3 (`<base>/dev_hdd0/home/00000001/savedata`), Cemu (`<base>/mlc01/usr/save`), mGBA (`<base>/*.sav`). Cada emulador vira um `DetectedGame` (`store = Emulador`). Para emuladores portáteis, `extra_roots` recebe as pastas onde o executável foi achado.
- Heurística (`heuristics.rs`): em `<winDocuments>/My Games/*`, `<home>/Saved Games/*`, `<winLocalAppDataLow>/*/*` e `<winAppData>/*`, pastas com arquivos de extensão `sav|save|dat|sl2|ess` ou nome contendo `save` que **não** casaram com nenhum jogo do manifesto viram `Source::Heuristica`. Ignorar `Microsoft`, `Google`, `Mozilla`, `NVIDIA Corporation`, `Packages`, `Temp`.

- [ ] **Step 1: Testes** (com `tempfile`, montando uma árvore falsa e um `PathContext` apontando para ela)
  - `acha_jogo_do_manifesto`: cria `<winAppData>/EldenRing/76561198025000000/ER0000.sl2`, Elden Ring instalado na Steam → 1 jogo, `source == Manifesto`, `files.len() == 1`
  - `jogo_sem_pasta_nao_aparece`: Celeste no manifesto, sem pasta → não está em `games`
  - `emulador_pcsx2`: cria `<winDocuments>/PCSX2/memcards/Mcd001.ps2` → jogo `pcsx2`, `store == Emulador`
  - `heuristica_pasta_desconhecida`: cria `<winDocuments>/My Games/JogoIndie/slot1.sav` → `source == Heuristica`, `name == "JogoIndie"`
  - `acima_do_limite`: jogo com `total_bytes` > limite (simular com limite injetado via `scan_with_limit(..., limit: u64)`, `pub(crate)`) vai para `over_limit` e não para `games`
  - `game_id_sem_acento`: `game_id("Pokémon Emerald") == "pokemon-emerald"`
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar. Globs com `globset` sobre `walkdir` a partir do prefixo fixo do padrão (nunca andar a partir de `C:/`).
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(motor): varre o PC atrás de saves (manifesto, emuladores e heurística)`.

### Task 6: Proteção existente e prioridade

**Files:** Create `crates/core/src/protection.rs`, `crates/core/src/sync_folders.rs`.

**Interfaces:**
- Consumes: `DetectedGame`, `Store`, `CloudFlags` (Tasks 3 e 5); `LibraryIndex` (Task 7; aqui só a leitura de `last_backup(game_id) -> Option<DateTime<Utc>>`, então a Task 6 define o trait `BackupHistory`, que a Task 7 implementa).
- Produces:
  - `pub enum SyncProvider { OneDrive, GoogleDrive, Dropbox }`
  - `pub struct SyncRoot { pub provider: SyncProvider, pub path: PathBuf }`
  - `#[cfg(windows)] pub fn detect_sync_roots() -> Vec<SyncRoot>`: OneDrive em `HKCU\Software\Microsoft\OneDrive\Accounts\*\UserFolder`; Dropbox em `%APPDATA%\Dropbox\info.json` e `%LOCALAPPDATA%\Dropbox\info.json` (`personal.path`/`business.path`); Google Drive em `HKCU\Software\Google\DriveFS\Share` → letra montada, ou `HKCU\Software\Google\DriveFS` `PerAccountPreferences` (`mount_point_path`)
  - `pub trait BackupHistory { fn last_backup(&self, game_id: &str) -> Option<DateTime<Utc>>; }`
  - `pub enum Protection { NuvemDaLoja(Store), PastaSincronizada(SyncProvider), BackupDoApp(DateTime<Utc>) }`
  - `pub enum Priority { Alta, Media, Baixa }`
  - `pub struct Assessment { pub priority: Priority, pub protections: Vec<Protection> }`
  - `pub fn assess(game: &DetectedGame, sync_roots: &[SyncRoot], history: &dyn BackupHistory, now: DateTime<Utc>) -> Assessment`
- Regras:
  - Nuvem da loja **confirmada** = a flag de nuvem da loja de onde o jogo veio é `true` (Steam + `cloud.steam`, Epic + `cloud.epic`, GOG + `cloud.gog`). Jogo Xbox conta sempre como confirmado.
  - **Não confirmada** = flag `true` de outra loja, mas o jogo veio de outra (ou `SemLoja`).
  - `PastaSincronizada` quando **todas** as `save_roots` estão dentro de um `SyncRoot` (ignorando maiúsculas).
  - Prioridade: Baixa se houver nuvem confirmada, pasta sincronizada ou backup com até 7 dias; Média se houver só nuvem não confirmada ou backup com mais de 7 dias; senão Alta.
- Ordenação: `pub fn sort_for_display(items: &mut [(DetectedGame, Assessment)])`: Alta → Média → Baixa, e depois pelo nome.

- [ ] **Step 1: Testes**
  - `sem_nada_e_alta`
  - `steam_com_cloud_e_baixa`
  - `gog_com_flag_so_steam_e_media`
  - `dentro_do_onedrive_e_baixa`: save em `C:/Users/João/OneDrive/Documentos/My Games/X`, `SyncRoot` em `C:/Users/joão/OneDrive` → Baixa (testa maiúsculas)
  - `backup_8_dias_e_media`, `backup_6_dias_e_baixa`
  - `ordena_alta_primeiro`
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(motor): calcula prioridade e detecta saves já protegidos`.

### Task 7: Motor de backup (versões, índice, retenção)

**Files:** Create `crates/core/src/backup/{mod,snapshot,library,retention}.rs`.

**Interfaces:**
- Consumes: `DetectedGame` (Task 5), `contract`, `PathContext` (Task 2), trait `BackupHistory` (Task 6).
- Produces:
  - `pub struct SnapshotMeta { pub game_id: String, pub game_name: String, pub created: DateTime<Utc>, pub source_pc: String, pub files: Vec<FileEntry>, pub total_bytes: u64, pub content_hash: String, pub pinned: bool, pub store: Store, pub store_user_id: Option<String> }`
  - `pub struct FileEntry { pub template: String /* caminho com marcadores */, pub zip_path: String, pub size: u64, pub blake3: String }`
  - `pub struct LibraryIndex { pub version: u32 /* = 1 */, pub games: BTreeMap<String, Vec<SnapshotRef>> }`, `pub struct SnapshotRef { pub stem: String /* "2026-11-20_21-40-05" */, pub created: DateTime<Utc>, pub pinned: bool, pub content_hash: String }`
  - `impl LibraryIndex { pub fn load(dest_root: &Path) -> Result<Self, CoreError>; pub fn save_atomic(&self, dest_root: &Path) -> Result<(), CoreError>; }`, que implementa `BackupHistory`
  - `pub enum BackupOutcome { Criado(SnapshotRef), SemMudanca }`
  - `pub fn backup_game(game: &DetectedGame, ctx: &PathContext, dest_root: &Path, keep: usize, now: DateTime<Utc>) -> Result<BackupOutcome, CoreError>`
  - `pub fn set_pinned(dest_root: &Path, game_id: &str, stem: &str, pinned: bool) -> Result<(), CoreError>`
  - `pub enum CoreError` (com `thiserror`; definido em `lib.rs` na Task 2 e estendido aqui): `ArquivoTravado(PathBuf)`, `DestinoIndisponivel(PathBuf)`, `Io(std::io::Error)`, `Formato(String)`, `CaminhoInseguro(String)`
- Algoritmo de `backup_game`:
  1. `content_hash` = BLAKE3 de (template ordenado + blake3 de cada arquivo). Se for igual ao da última versão → `SemMudanca`.
  2. Abrir cada arquivo só para leitura. Erro de compartilhamento (`ERROR_SHARING_VIOLATION`, os error 32) → `ArquivoTravado`, e nada é gravado.
  3. Gravar `SaveGuard/<id>/<stem>.zip.tmp` e `<stem>.json.tmp`, fazer `sync_all` e renomear para os nomes finais.
  4. Atualizar o índice e gravar com `save_atomic` (`biblioteca.json.tmp` → rename).
  5. Retenção: apagar as versões mais antigas não fixadas além de `keep`, só **depois** do passo 4.

- [ ] **Step 1: Testes** (tempdir como destino)
  - `cria_versao_e_indice`: 1 jogo com 2 arquivos → `Criado`; existem o `.zip`, o `.json` e o `biblioteca.json`; o zip tem 2 entradas; `json.files[0].template` começa com `<`
  - `sem_mudanca_nao_cria`: dois backups seguidos → o segundo é `SemMudanca`
  - `retencao_mantem_10_e_fixadas`: 12 backups com conteúdo diferente, o 1º fixado → sobram 11 (10 mais recentes + a fixada)
  - `arquivo_travado_nao_grava`: abrir um arquivo com `share_mode(0)` (`#[cfg(windows)]`) → `Err(ArquivoTravado)` e nenhum `.zip` novo nem mudança no índice
  - `indice_corrompido_nao_some`: simular falha entre `.zip.tmp` e o rename (injetar erro via `backup_game_with(io: &dyn Fs)`, `pub(crate)`) → `biblioteca.json` continua igual ao anterior e é JSON válido
  - `destino_inexistente`: `dest_root = "Z:/nao-existe"` → `Err(DestinoIndisponivel)`
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(backup): cria versões zip com índice atômico e retenção`.

### Task 8: Destinos e configuração

**Files:** Create `crates/core/src/destinations.rs`, `crates/core/src/config.rs`.

**Interfaces:**
- Consumes: `SyncRoot`, `detect_sync_roots` (Task 6).
- Produces:
  - `pub enum DestinationKind { PastaSincronizada(SyncProvider), DiscoRemovivel, PastaLocal }`
  - `pub struct Destination { pub id: String, pub label: String, pub kind: DestinationKind, pub root: PathBuf, pub available: bool, pub free_bytes: Option<u64>, pub has_backup: bool }`
  - `pub fn list_destinations(cfg: &Config) -> Vec<Destination>`: pastas sincronizadas (raiz `<SyncRoot>/SaveGuard Backups`) + discos removíveis (`GetDriveTypeW == DRIVE_REMOVABLE` ou disco USB fixo via `GetDriveTypeW == DRIVE_FIXED` e barramento USB, usando `IOCTL_STORAGE_QUERY_PROPERTY`) + pastas adicionadas pelo usuário; `has_backup` = existe `SaveGuard/biblioteca.json`
  - `pub struct Config { pub version: u32, pub auto_destinations: Vec<String>, pub custom_folders: Vec<PathBuf>, pub keep_versions: usize /* 10 */, pub per_game_keep: BTreeMap<String, usize>, pub manual_games: Vec<ManualGame>, pub ignored_games: Vec<String>, pub theme: String /* "gamer" */ }`
  - `impl Config { pub fn load_or_default(path: &Path) -> Config; pub fn save(&self, path: &Path) -> Result<(), CoreError>; }`. Caminho real: `%APPDATA%\SaveGuard\config.json`. Gravação atômica.
- [ ] **Step 1: Testes**
  - `config_padrao`: arquivo inexistente → `keep_versions == 10`, `theme == "gamer"`
  - `config_ida_e_volta`
  - `config_corrompida_usa_padrao_e_guarda_copia`: JSON inválido → padrão, e o arquivo original é renomeado para `config.json.corrompido`
  - `pasta_custom_marca_has_backup`: tempdir com `SaveGuard/biblioteca.json` → `has_backup == true`
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(backup): lista destinos (nuvem, discos externos, pastas) e salva configuração`.

### Task 9: Restauração

**Files:** Create `crates/core/src/restore.rs`.

**Interfaces:**
- Consumes: `LibraryIndex`, `SnapshotMeta`, `backup_game` (Task 7); `expand`, `PathContext` (Task 2); `Manifest`, `InstalledGame` (para achar `<base>` do jogo no PC novo).
- Produces:
  - `pub struct RestoreItem { pub zip_path: String, pub target: Option<PathBuf> /* None = não resolvido */, pub exists: bool, pub existing_modified: Option<DateTime<Utc>> }`
  - `pub struct RestorePlan { pub game_id: String, pub stem: String, pub items: Vec<RestoreItem>, pub unresolved: usize, pub conflicts: usize }`
  - `pub fn plan_restore(dest_root: &Path, game_id: &str, stem: &str, ctx: &PathContext, vars: &GameVars) -> Result<RestorePlan, CoreError>`
  - `pub fn apply_restore(plan: &RestorePlan, dest_root: &Path, ctx: &PathContext, overrides: &BTreeMap<String, PathBuf>, now: DateTime<Utc>) -> Result<RestoreReport, CoreError>`
  - `pub struct RestoreReport { pub restored: usize, pub safety_snapshot: Option<SnapshotRef> }`
- Regras:
  - Antes de escrever, se `conflicts > 0`, faz um backup de segurança dos alvos existentes no mesmo destino (versão **fixada** automaticamente).
  - Cada `zip_path` é rejeitado com `CaminhoInseguro` se contiver `..`, for absoluto ou tiver prefixo de unidade.
  - O alvo final precisa estar dentro do caminho expandido do template (sem `..` depois de normalizar).
  - `<storeUserId>` vem do `PathContext` do PC novo.
- [ ] **Step 1: Testes**
  - `restaura_em_pc_novo`: backup feito com ctx A (`home = .../Ana`, `store_user_id = "111"`), restauração com ctx B (`home = .../Bruno`, `store_user_id = "222"`) → arquivos em `.../Bruno/AppData/Roaming/EldenRing/222/...`
  - `sem_base_fica_nao_resolvido`: template com `<base>` e `vars.base = None` → `unresolved == 1`
  - `conflito_cria_backup_fixado`: alvo já existe → `safety_snapshot.is_some()` e o `SnapshotRef` correspondente tem `pinned == true`
  - `zip_slip_rejeitado`: zip montado à mão com entrada `../../evil.txt` → `Err(CaminhoInseguro)` e nada escrito
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(backup): restaura saves no PC novo com backup de segurança`.

### Task 10: Comandos Tauri e interface mínima (tema gamer)

**Files:**
- Create: `src-tauri/src/commands.rs`, `src-tauri/src/state.rs`; Modify: `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`
- Create: `src/lib/api.ts`, `src/lib/types.ts`, `src/lib/format.ts`, `src/theme/gamer.css`, `src/App.svelte`, `src/screens/{Inicio,Jogos,Restaurar,Sobre}.svelte`, `src/components/{GameRow,PriorityBadge,DestinationPicker,ScanButton}.svelte`
- Test: `src/lib/format.test.ts`, `src/components/DestinationPicker.test.ts`

**Interfaces:**
- Consumes: tudo de `saveguard-core`.
- Produces (comandos Tauri; nomes e tipos espelhados em `src/lib/types.ts`):
  - `scan() -> Vec<GameView>`, onde `GameView { id, name, store, source, priority, protections: Vec<String>, files: usize, total_bytes, last_backup: Option<String>, save_roots: Vec<String> }`. Os caminhos já vêm contraídos com marcadores.
  - `list_destinations() -> Vec<Destination>`
  - `add_custom_folder() -> Option<Destination>` (abre o seletor de pasta do sistema pelo plugin `dialog`)
  - `backup_now(game_ids: Vec<String>, destination_ids: Vec<String>) -> Vec<BackupResultView>`, onde `BackupResultView { game_id, destination_id, status: "criado"|"sem_mudanca"|"erro", message: Option<String> }`
  - `list_snapshots(destination_id: String) -> Vec<GameSnapshotsView>`
  - `plan_restore(destination_id, game_id, stem) -> RestorePlan`, `apply_restore(destination_id, game_id, stem, overrides) -> RestoreReport`
  - `confirm_heuristic(game_id: String, keep: bool)`
- O estado guarda o último `ScanResult` e o `Config`. A varredura roda em `tauri::async_runtime::spawn_blocking`.
- Capabilities: só os comandos acima + `dialog:allow-open`. Sem `shell` e sem `fs` no frontend.
- Interface:
  - **Início:** botão "Escanear PC", 3 cartões ("N jogos encontrados", "N sem cópia fora do PC", "Último backup: …") e a lista com `GameRow`: nome, selo da loja, selo de prioridade (Alta vermelho, Média âmbar, Baixa verde), proteções como chips, tamanho e checkbox de seleção. Botões "Fazer backup dos selecionados" e "Fazer backup de todos em risco".
  - **Itens de heurística:** mostrados com o selo "Possível save — confirmar" e os botões Confirmar/Descartar.
  - **Itens acima de 2 GB:** seção recolhida "Grandes demais — confirmar" com o tamanho.
  - **DestinationPicker** (abre a cada backup manual): lista de destinos com ícone, espaço livre e "já tem backup"; os indisponíveis aparecem desabilitados; botão "Adicionar pasta…"; nenhum pré-marcado se `auto_destinations` estiver vazio.
  - **Restaurar:** escolher destino → lista de jogos com versões (data, tamanho, 📌 fixada) → "Restaurar" mostra o plano (resolvidos, conflitos, não resolvidos com botão "Escolher pasta…") → confirmar.
  - **Sobre:** versão e crédito do ludusavi-manifest/PCGamingWiki.
  - **Tema:** tokens CSS do `design/stitch/v0-dashboard-original.png` (fundo `#0f1412`, superfície `#161d1a`, destaque `#19e6a0`, alerta `#e5484d`, fonte Space Grotesk/Inter). Todos os textos via `{}` do Svelte, nunca `{@html}`.
- [ ] **Step 1: Testes**
  - `format.test.ts`: `formatBytes(18_400_000) == "18,4 MB"`, `formatRelative(agora - 4min) == "há 4 minutos"`
  - `DestinationPicker.test.ts`: destino com `available: false` renderiza desabilitado; com `auto_destinations` vazio, nenhum checkbox vem marcado; o botão "Copiar" fica desabilitado sem seleção
  - Rust: `commands::tests::game_view_contrai_caminhos`: `GameView.save_roots[0]` começa com `<`
- [ ] **Step 2:** Rodar `npm run test -- --run` e `cargo test -p saveguard`. Esperado: FAIL.
- [ ] **Step 3:** Implementar comandos, estado e telas.
- [ ] **Step 4:** Rodar os testes. Esperado: PASS. Depois `npm run tauri dev`: escanear o PC real, fazer backup de 1 jogo para uma pasta de teste e restaurar numa pasta renomeada. Registrar no PR o que foi testado de verdade e abrir issue `testar` para o que não deu para testar.
- [ ] **Step 5:** Commit `feat(interface): adiciona telas de varredura, backup manual e restauração`.
- [ ] **Step 6:** Push da branch e PR para `dev` com título `feat: fundação do SaveGuard (varredura, backup manual e restauração)`, corpo com "Parte de #1" e os itens do checklist de segurança cobertos. Merge só com o ok do usuário.
