# SaveGuard — Plano 2: backup contínuo, bandeja e pendrive

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** O app copia os saves sozinho quando o jogo fecha (ou quando a pasta do save para de mudar), fica na bandeja, inicia com o Windows e oferece copiar tudo ao plugar um pendrive/HD, guardando numa fila o que não puder ser enviado na hora.

**Architecture:** A lógica de decisão (quando fazer backup, qual jogo é qual processo, qual pasta é de qual jogo, fila de pendentes, destinos automáticos) fica em `crates/core/src/watch/` e `crates/core/src/pending.rs`, pura e testável, com o tempo injetado em segundos monotônicos. O app Tauri ganha um motor em thread própria (`src-tauri/src/auto.rs`) que alimenta essa lógica com `sysinfo` (processos), `notify` (pastas) e a lista de discos externos, e executa os backups com uma trava por destino. A bandeja, o início com o Windows, a instância única e as notificações usam plugins oficiais do Tauri 2.

**Tech Stack:** o do plano 1 + crates `sysinfo`, `notify`; recursos `tray-icon` e `image-png` do `tauri`; plugins `tauri-plugin-autostart`, `tauri-plugin-single-instance`, `tauri-plugin-notification`.

**Spec:** `docs/superpowers/specs/2026-11-20-saveguard-design.md` (seções 5.3, 5.5, 5.7). Issue: #5 (parte de #1).

## Global Constraints

- Processos consultados a cada **5 s**; backup **10 s** depois de o jogo fechar; vigia de pasta dispara depois de **120 s** sem mudanças.
- Com o jogo aberto, o vigia **só marca** a pasta como mudada; a cópia acontece ao fechar.
- Backup automático vai para `Config.auto_destinations`. Destino indisponível → fila em `%LOCALAPPDATA%\SaveGuard\pendentes\<id do destino em hash>`, enviada quando ele voltar.
- Backup manual continua perguntando o destino (plano 1, sem mudança).
- Iniciar com o Windows **ligado por padrão**, abrindo minimizado na bandeja (`--minimizado`). Fechar a janela esconde na bandeja; "Sair" só no menu da bandeja.
- Uma instância só: abrir de novo traz a janela existente.
- A cópia automática roda em **prioridade baixa** de CPU e disco (`THREAD_MODE_BACKGROUND_BEGIN`).
- Nunca mais de um backup ao mesmo tempo no mesmo destino (trava por destino).
- Nada de notificação por backup bem-sucedido (sem spam enquanto joga); notifica erro, disco plugado e envio de pendentes.
- Mesmo padrão de commits, idioma e segurança do plano 1. Branch `feature/5-backup-continuo`; PR para `dev` com "Fecha #5" e "Parte de #1".

## Review Focus

1. **Jogo fecha por crash ou é morto pelo Gerenciador de Tarefas:** o processo some do mesmo jeito → o backup acontece igual. Teste na Task 1 (`parar_sem_evento_de_saida_tambem_agenda`, via tracker na Task 2).
2. **Pasta de save apagada ou renomeada enquanto é vigiada** (jogo desinstalado): o motor não pode morrer; o vigia ignora o erro e segue. Teste na Task 5 (`motor_segue_quando_pasta_some`).
3. **PC suspenso / relógio do Windows ajustado:** o agendador usa segundos monotônicos (`Instant`), nunca a hora do relógio. Teste na Task 1 (`tempo_monotonico_nao_volta`).
4. **Pendrive arrancado no meio do backup automático:** a falha de destino vira pendente, não perde a versão. Teste na Task 5 (`destino_some_vira_pendente`).
5. **Sem permissão para registrar o início com o Windows:** o app abre normalmente e só registra o aviso. Coberto na Task 6 (erro do plugin vira log, nunca `expect`).

---

### Task 1: Agendador de backups automáticos

**Files:** Create `crates/core/src/watch/mod.rs`, `crates/core/src/watch/scheduler.rs`; Modify `crates/core/src/lib.rs`.

**Interfaces:**
- Produces:
  - `pub const ESPERA_APOS_FECHAR: u64 = 10;` `pub const SILENCIO_DA_PASTA: u64 = 120;` (segundos)
  - `pub enum WatchEvent { JogoAbriu(String), JogoFechou(String), PastaMudou(String) }`
  - `pub struct Scheduler` com `pub fn new() -> Self`, `pub fn on_event(&mut self, ev: WatchEvent, now: u64)`, `pub fn due(&mut self, now: u64) -> Vec<String>` (devolve e remove os ids vencidos, ordenados), `pub fn running(&self) -> Vec<String>`, `pub fn pending_count(&self) -> usize`.
- Regras: `JogoFechou` agenda para `now + 10`; `PastaMudou` com o jogo fechado agenda para `now + 120` (cada mudança empurra o prazo); com o jogo aberto só marca `dirty`; `JogoAbriu` cancela o agendamento por pasta; `now` menor que o último visto é tratado como o último (tempo nunca volta).

- [ ] **Step 1: Testes**
  - `fechar_agenda_em_10s`: `JogoFechou("a")` em 100 → `due(109)` vazio, `due(110) == ["a"]`, e `due(111)` vazio (não repete)
  - `pasta_mudando_empurra_o_prazo`: `PastaMudou("a")` em 0 e em 60 → `due(150)` vazio, `due(180) == ["a"]`
  - `com_jogo_aberto_pasta_so_marca`: `JogoAbriu("a")` em 0, `PastaMudou("a")` em 10 → `due(500)` vazio; `JogoFechou("a")` em 600 → `due(610) == ["a"]`
  - `abrir_cancela_agendamento_da_pasta`: `PastaMudou` em 0, `JogoAbriu` em 30 → `due(200)` vazio
  - `parar_sem_evento_de_saida_tambem_agenda`: `JogoAbriu` e depois `JogoFechou` sem nada no meio (crash) → agenda normalmente
  - `tempo_monotonico_nao_volta`: `JogoFechou` em 100, `due(50)` vazio, `due(110) == ["a"]`
  - `running_e_pendentes`: depois de `JogoAbriu("a")` e `PastaMudou("b")` → `running() == ["a"]`, `pending_count() == 1`
- [ ] **Step 2:** `cargo test -p saveguard-core watch::scheduler`. Esperado: FAIL (não compila).
- [ ] **Step 3:** Implementar com `BTreeMap<String, u64>` (prazos) e `BTreeSet<String>` (abertos, dirty).
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(motor): agenda backups automáticos ao fechar o jogo ou a pasta parar de mudar`.

### Task 2: Qual processo é qual jogo e qual pasta é de qual jogo

**Files:** Create `crates/core/src/watch/process.rs`, `crates/core/src/watch/folders.rs`.

**Interfaces:**
- Consumes: `DetectedGame` (com `base`, `exe_hint`, `save_roots`), `norm` de `paths`.
- Produces:
  - `pub struct ProcInfo { pub pid: u32, pub exe: PathBuf }`
  - `pub struct ProcessTracker` com `pub fn new(games: &[DetectedGame]) -> Self` e `pub fn update(&mut self, procs: &[ProcInfo]) -> Vec<WatchEvent>` (só `JogoAbriu`/`JogoFechou`, na ordem dos ids).
  - Um processo é do jogo quando `exe == exe_hint` ou o `exe` está **dentro** de `base` (sem diferenciar maiúsculas). Jogos sem `base` nem `exe_hint` não são rastreados por processo (o vigia de pasta cobre).
  - `pub fn games_for_path(path: &Path, games: &[DetectedGame]) -> Vec<String>`: ids cujos `save_roots` contêm o caminho.
  - `pub fn watch_roots(games: &[DetectedGame]) -> Vec<PathBuf>`: `save_roots` sem duplicados e sem pastas contidas em outras.
- [ ] **Step 1: Testes**
  - `exe_dentro_da_pasta_do_jogo`: jogo com `base = D:/Jogos/Hades`; processo `D:/jogos/hades/x64/Hades.exe` → `[JogoAbriu("hades")]`; mesma lista de novo → `[]`; lista vazia → `[JogoFechou("hades")]`
  - `exe_hint_de_emulador`: `exe_hint = E:/Emu/pcsx2-qt.exe` casa só com esse exe
  - `prefixo_parecido_nao_casa`: `D:/Jogos/Hades2/h.exe` não é do jogo com base `D:/Jogos/Hades`
  - `dois_processos_do_mesmo_jogo_um_evento`: launcher + jogo na mesma base → um `JogoAbriu`; fecha só quando os dois somem
  - `jogo_sem_base_nao_rastreia`: jogo sem `base`/`exe_hint` nunca gera eventos
  - `pasta_pertence_ao_jogo`: `games_for_path(<save_root>/sub/a.sav)` → `["x"]`; caminho fora → `[]`
  - `watch_roots_sem_repetidos`: `[C:/A, C:/A/B, C:/c]` → `[C:/A, C:/c]`
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(motor): associa processos e pastas aos jogos detectados`.

### Task 3: Fila de pendentes e limpeza de temporários

**Files:** Create `crates/core/src/pending.rs`; Modify `crates/core/src/backup/mod.rs`.

**Interfaces:**
- Consumes: `backup_game`, `LibraryIndex` (que já se reconstrói pelos `.json`), `backup_root`.
- Produces:
  - `pub fn pending_root(base: &Path, destination_id: &str) -> PathBuf`: `base/<blake3(id)[..16]>`; a pasta guarda um `destino.txt` com o id.
  - `pub fn pending_count(pending_root: &Path) -> usize`: versões (`.zip` com `.json`) esperando envio.
  - `pub fn flush_pending(pending_root: &Path, dest_root: &Path) -> Result<usize, CoreError>`: copia cada `<jogo>/<versão>.zip/.json` que o destino ainda não tem (zip primeiro, depois json, com `.tmp` + rename), regrava o índice do destino e só então apaga do pendente; devolve quantas versões foram enviadas.
  - `pub fn cleanup_temp(dest_root: &Path) -> usize` em `backup`: apaga `*.tmp` dentro de `SaveGuard/` e `SaveGuard/*/`; devolve quantos.
- [ ] **Step 1: Testes**
  - `pendente_vai_para_o_destino_quando_volta`: backup em `pending_root`, `flush_pending` para um destino vazio → `1`; destino lista a versão no `LibraryIndex`; pendente fica vazio (`pending_count == 0`)
  - `nao_duplica_o_que_o_destino_ja_tem`: mesma versão já no destino → `flush_pending == 0` e o pendente é limpo
  - `destino_indisponivel_mantem_pendente`: `flush_pending(p, "Z:/nada")` → `Err(DestinoIndisponivel)` e `pending_count` igual
  - `ids_diferentes_pastas_diferentes`: `pending_root(b, "disco:1") != pending_root(b, "disco:2")` e ambos dentro de `b`
  - `limpa_tmp_orfao`: cria `SaveGuard/x/a.zip.tmp` e `SaveGuard/biblioteca.json.tmp` → `cleanup_temp == 2` e versões válidas intactas
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(backup): guarda backups pendentes e envia quando o destino volta`.

### Task 4: Configuração do modo automático e destinos automáticos

**Files:** Modify `crates/core/src/config.rs`, `crates/core/src/destinations.rs`.

**Interfaces:**
- Produces:
  - `Config` ganha `autostart: bool` (padrão `true`) e `auto_paused: bool` (padrão `false`); configs antigas carregam com os padrões.
  - `pub struct AutoTargets { pub available: Vec<Destination>, pub unavailable: Vec<String> }`
  - `pub fn auto_targets(cfg: &Config, all: &[Destination]) -> AutoTargets`: ids de `auto_destinations` que estão em `all` e disponíveis vão em `available`; os demais (desplugados ou sumidos) em `unavailable`.
- [ ] **Step 1: Testes**
  - `config_antiga_ganha_padroes`: JSON sem os campos novos → `autostart == true`, `auto_paused == false`
  - `alvos_separa_disponiveis`: auto = `[sync:a, disco:1, pasta:x]`, só `sync:a` disponível e `disco:1` presente mas indisponível, `pasta:x` ausente → `available` = `[sync:a]`, `unavailable` = `[disco:1, pasta:x]`
  - `sem_destinos_automaticos_vazio`
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(backup): configura destinos e pausa do backup automático`.

### Task 5: Motor automático no app

**Files:** Create `src-tauri/src/auto.rs`; Modify `src-tauri/src/state.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`.

**Interfaces:**
- Consumes: Tasks 1–4; `backup_game`, `list_destinations`, `ScanData`.
- Produces:
  - `pub struct DestLocks` (`Mutex<HashMap<String, Arc<Mutex<()>>>>`) com `pub fn lock(&self, id: &str) -> Arc<Mutex<()>>`, guardado no `AppState`.
  - `pub enum AutoOutcome { Copiado { game_id: String, destination_id: String }, SemMudanca { .. }, Pendente { game_id, destination_id }, Erro { game_id, message } }`
  - `pub fn execute_due(ids: &[String], scan: &ScanData, cfg: &Config, dests: &[Destination], pending_base: &Path, locks: &DestLocks, now: DateTime<Utc>) -> Vec<AutoOutcome>`: para cada jogo e destino automático disponível faz `backup_game` com a trava; destino indisponível ou erro de destino (`DestinoIndisponivel`/`Io`) → backup no `pending_root` daquele destino (`Pendente`); arquivo travado/mudando → `Erro` (tenta de novo no próximo fechamento).
  - `pub fn flush_all(cfg: &Config, dests: &[Destination], pending_base: &Path, locks: &DestLocks) -> usize`: envia pendentes dos destinos automáticos disponíveis.
  - `pub fn spawn(app: AppHandle)`: thread que, a cada 5 s, lê processos (`sysinfo`, só `exe`), alimenta `ProcessTracker` e `Scheduler`, recebe eventos do `notify` (vigia recriado quando a varredura muda; erro do vigia é registrado e ignorado), executa `due` com `execute_due` em prioridade baixa (`SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN)`), chama `flush_all` a cada 60 s, emite o evento `status-automatico` (`AutoStatusView`) e atualiza a bandeja. Respeita `auto_paused`. Faz uma varredura ao iniciar (mesmo código do comando `scan`, extraído para `fn run_scan(state) -> Result<Vec<GameView>, String>`).
  - `#[derive(Serialize)] pub struct AutoStatusView { pub paused: bool, pub running: Vec<String>, pub waiting: usize, pub pending: usize, pub last_backup: Option<String>, pub at_risk: usize, pub copying: bool }`
- [ ] **Step 1: Testes** (em `auto.rs`, com tempdirs)
  - `execute_due_copia_para_destinos_automaticos`: 1 jogo, auto = 1 pasta disponível → `[Copiado]` e a versão existe lá
  - `destino_some_vira_pendente`: auto = pasta que não existe mais → `[Pendente]` e `pending_count == 1`; depois a pasta passa a existir e `flush_all == 1`
  - `sem_destino_automatico_nao_faz_nada`: auto vazio → `[]`
  - `trava_por_destino_serializa`: duas threads com `execute_due` no mesmo destino ao mesmo tempo → ambas terminam e o `LibraryIndex` tem as duas versões (índice não perde atualização)
  - `motor_segue_quando_pasta_some`: `watch_roots` com pasta inexistente → `try_watch` devolve os erros sem pânico e vigia as que existem
- [ ] **Step 2:** Rodar `cargo test -p saveguard auto`. Esperado: FAIL.
- [ ] **Step 3:** Implementar (adicionar `sysinfo`, `notify`, feature `Win32_System_Threading` no `windows-sys` do app).
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(bandeja): adiciona o motor de backup automático`.

### Task 6: Bandeja, iniciar com o Windows e instância única

**Files:** Create `src-tauri/src/tray.rs`; Modify `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/capabilities/default.json`, `src-tauri/tauri.conf.json`.

**Interfaces:**
- Produces:
  - `pub enum TrayState { Protegido, EmRisco, Copiando, Pausado }` e `pub fn tray_icon_rgba(state: TrayState) -> (Vec<u8>, u32, u32)`: ícone 32×32 gerado no código (escudo do app com um ponto de cor: verde, vermelho, azul, cinza).
  - `pub fn tray_tooltip(status: &AutoStatusView) -> String`, ex. `"SaveGuard — protegido · último backup há 5 min"`, `"SaveGuard — 2 jogos sem cópia fora do PC"`, `"SaveGuard — pausado"`.
  - `pub fn build(app: &AppHandle) -> tauri::Result<()>`: bandeja com menu **Fazer backup agora** (todos os jogos → destinos automáticos), **Abrir SaveGuard**, **Pausar/Retomar backup automático**, itens dinâmicos **Copiar tudo para <disco>** (Task 7), **Sair**. Clique no ícone abre a janela.
  - Fechar a janela → `hide()` + `prevent_close()`. `--minimizado` nos argumentos → janela começa escondida (`"visible": false` no conf e `show()` só sem a flag).
  - `tauri-plugin-single-instance`: segunda execução mostra e foca a janela.
  - `tauri-plugin-autostart` com `args(["--minimizado"])`: na inicialização, sincroniza com `Config.autostart` (liga/desliga); erro vira `eprintln!`, nunca `panic`.
- [ ] **Step 1: Testes**
  - `icone_tem_32x32_rgba`: `tray_icon_rgba(Protegido)` → `(v, 32, 32)` com `v.len() == 4096`
  - `estados_tem_cores_diferentes`: pixel do ponto difere entre os 4 estados
  - `tooltip_por_estado`: pausado → contém "pausado"; `at_risk = 2` → contém "2 jogos sem cópia"; senão "protegido"
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar; capabilities ganham só `autostart:default` (nada de shell/fs). Comandos novos (Task 8) entram em `build.rs` e nas capabilities.
- [ ] **Step 4:** Rodar os testes; `npm run tauri dev`: fechar a janela deixa o ícone na bandeja, menu abre/pausa/sai, segunda execução foca a janela.
- [ ] **Step 5:** Commit `feat(bandeja): fica na bandeja, inicia com o Windows e abre uma instância só`.

### Task 7: Pendrive/HD plugado

**Files:** Create `src-tauri/src/drives.rs`; Modify `src-tauri/src/auto.rs`, `src-tauri/src/tray.rs`.

**Interfaces:**
- Produces:
  - `pub struct DriveWatcher` com `pub fn update(&mut self, now_present: &[Destination]) -> Vec<DriveEvent>` considerando só `DestinationKind::DiscoRemovivel`; `pub enum DriveEvent { Plugado(Destination), Removido(String) }`. O primeiro `update` (discos que já estavam plugados ao abrir o app) não gera `Plugado`.
  - No motor: a cada 5 s consulta os destinos; `Plugado` → notificação "PENDRIVE (E:) conectado — abra o SaveGuard para copiar seus saves" (ou "atualizar o backup" se `has_backup`), evento `disco-plugado` para a interface, item no menu da bandeja e `flush_all` se for destino automático; `Removido` → evento `disco-removido`, item sai do menu.
  - Comando `copy_all_to(destination_id) -> Vec<BackupResultView>`: backup de todos os jogos da última varredura (exceto possíveis saves não confirmados) para aquele destino, com a trava.
- [ ] **Step 1: Testes**
  - `primeira_leitura_nao_avisa`
  - `plugar_e_remover`: `[]` → `[E]` gera `Plugado(E)`; `[E]` → `[]` gera `Removido(id de E)`
  - `ignora_nuvem_e_pastas`: destinos de outros tipos nunca geram evento
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar; adicionar `tauri-plugin-notification` (permissão `notification:default`).
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(bandeja): avisa ao plugar pendrive ou HD e copia tudo com um clique`.

### Task 8: Interface — Configurações, status e aviso de disco

**Files:** Create `src/screens/Configuracoes.svelte`, `src/components/StatusAutomatico.svelte`, `src/components/AvisoDisco.svelte`, `src/screens/Configuracoes.test.ts`; Modify `src/App.svelte`, `src/screens/Inicio.svelte`, `src/lib/api.ts`, `src/lib/types.ts`, `src-tauri/src/commands.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/default.json`.

**Interfaces:**
- Comandos novos: `get_settings() -> SettingsView { auto_destinations, autostart, auto_paused, keep_versions }`, `set_auto_destinations(ids: Vec<String>)` (só ids que existem em `list_destinations`), `set_autostart(enabled: bool)` (atualiza config e o plugin), `set_paused(paused: bool)`, `auto_status() -> AutoStatusView`, `copy_all_to(destination_id)` (Task 7).
- **Configurações:** "Destinos do backup automático" (checkbox por destino, mesmo visual do seletor; aviso "Nenhum destino escolhido: o backup automático não vai copiar nada" quando vazio), "Iniciar com o Windows", "Pausar backup automático", "Versões guardadas por jogo" (número 1–50, só leitura neste plano se não houver comando; usar `keep_versions` exibido).
- **Início:** `StatusAutomatico` no banner ("Proteção contínua ativa · último backup há X", "Pausada", "Jogando agora: Hades — copio quando fechar", "N backups esperando o destino voltar"), atualizado pelo evento `status-automatico`. `AvisoDisco` aparece com o evento `disco-plugado`: "PENDRIVE (E:) conectado. Copiar todos os saves para ele?" [Copiar] [Agora não] (ou "Atualizar backup").
- Menu lateral ganha **Configurações** (⚙️).
- [ ] **Step 1: Testes** (`invoke` e `listen` mockados)
  - `marcar_destino_salva`: clicar no checkbox do OneDrive chama `set_auto_destinations` com `["sync:..."]`
  - `aviso_sem_destino`: `auto_destinations` vazio mostra o aviso
  - `alternar_iniciar_com_windows`: clicar chama `set_autostart(false)`
  - `aviso_disco_copiar`: renderizar `AvisoDisco` com um destino e clicar em **Copiar** chama `copy_all_to` com o id
- [ ] **Step 2:** Rodar `npm run test -- --run`. Esperado: FAIL.
- [ ] **Step 3:** Implementar telas e comandos.
- [ ] **Step 4:** Rodar os testes + `npm run check` + `cargo test --workspace`. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(interface): adiciona configurações do backup automático e aviso de disco`.

### Task 9: Verificação real e documentação

**Files:** Modify `README.md`; Create `crates/core/tests/automatico_real.rs` (ignorado por padrão).

- [ ] **Step 1:** Teste ignorado `ciclo_automatico_com_processo_real`: copia um save real para um tempdir, aponta um `DetectedGame` com `base` numa pasta temporária, inicia um processo de verdade a partir dela (`cmd.exe` copiado para lá com `timeout /t 3`), alimenta `ProcessTracker` + `Scheduler` com `sysinfo` real e confirma `JogoAbriu` → `JogoFechou` → backup criado no destino depois de 10 s.
- [ ] **Step 2:** Rodar com `--ignored --nocapture`. Esperado: PASS.
- [ ] **Step 3:** `npm run tauri dev`: escolher uma pasta de teste como destino automático, mexer num save (ex.: do Skate copiado para uma pasta manual), esperar 2 min e conferir a versão nova; fechar a janela e ver o ícone na bandeja. O que não der para testar vira issue `testar`.
- [ ] **Step 4:** README: seção "Backup automático" (como funciona, onde ficam os pendentes, como desligar o início com o Windows).
- [ ] **Step 5:** Commit `docs: explica o backup automático e a bandeja`. Push e PR para `dev` (`Fecha #5`, `Parte de #1`), com o checklist de segurança.
