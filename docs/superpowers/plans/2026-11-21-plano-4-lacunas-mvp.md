# Plano 4: lacunas do MVP e atualização automática

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fechar as funções do MVP que ficaram sem tela e preparar o SavesGG 0.2.0 para distribuição: restaurar vários jogos de uma vez, abrir uma pasta avulsa, fixar versões e escolher quantas guardar, jogo manual, descartados, último jogado, aviso do app da nuvem, log em arquivo e atualização automática assinada.

**Architecture:**
- A lógica pura fica em `crates/core`, que é testável sem Tauri.
- `src-tauri/src/commands.rs` ganha comandos finos que validam a entrada e chamam o core.
- A interface Svelte só acrescenta controles às telas que já existem.
- A atualização usa `tauri-plugin-updater`, com o `latest.json` publicado num repositório público de lançamentos.

**Tech Stack:**
- Rust: Tauri 2, `tauri-plugin-log`, `tauri-plugin-updater`, `tauri-plugin-process`, `sysinfo` 0.40.
- Interface: Svelte 5 (runes), Vitest e @testing-library/svelte.

**Spec:** `docs/superpowers/specs/2026-11-21-lacunas-mvp-design.md` (issue #18)

## Global Constraints

**Validação no Rust**
- Toda entrada da interface é validada no Rust:
  - nomes de jogo com 1 a 80 caracteres, depois de `trim`, e sem caracteres de controle;
  - número de versões entre 1 e 100;
  - `game_id` e `stem` precisam existir no índice do destino;
  - pastas só chegam pelo seletor do Windows (`app.dialog()`), nunca como texto da interface.

**Formato do backup**
- O formato só ganha campos com `#[serde(default)]`, para os arquivos do 0.1.0 continuarem sendo lidos.

**Registro de comandos**
- Cada comando Tauri novo é registrado em quatro lugares:
  - a função `#[tauri::command]` em `commands.rs`;
  - o `generate_handler!` em `src-tauri/src/lib.rs`;
  - a lista `commands` em `src-tauri/build.rs`;
  - `"allow-<nome-com-hifens>"` em `src-tauri/capabilities/default.json`.
- Também ganha uma entrada em `src/lib/api.ts`, e os tipos novos vão em `src/lib/types.ts`.

**Textos e telas**
- Textos em português do Brasil.
- Conteúdo do usuário sempre por interpolação Svelte, nunca `{@html}`.
- Os erros que vão para a interface são `String` curtas; o detalhe vai para o log (a partir da Tarefa 8).

**Verificação ao fim de cada tarefa**
- Precisam passar:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `npx vitest run`
  - `npx svelte-check`

**Commits**
- Conventional Commits em português, terminando com `Refs #18` e `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

**Endereços fixos**
- Repositório de lançamentos: `thelipesousa/savesgg-releases`.
- Endpoint da atualização: `https://github.com/thelipesousa/savesgg-releases/releases/latest/download/latest.json`.
- Versão: `0.2.0`.

## Review Focus

1. **Restaurar vários com um jogo já aberto ou um arquivo travado:** aquele jogo vira `Erro` com uma mensagem legível e os outros continuam (teste na Tarefa 2).
2. **Restaurar vários quando a única versão do jogo é de segurança:** o jogo não é restaurado com ela; vira `Erro("só há versões de segurança")` (teste na Tarefa 2).
3. **Pasta escolhida é a própria `SavesGG`, `SavesGG Backups` ou uma pasta antiga `SaveGuard`:** as três são aceitas e resolvem para a raiz certa (teste na Tarefa 3).
4. **Nome de jogo manual só com espaços, com 200 letras ou repetindo o id de outro jogo manual:** o app recusa com uma mensagem clara e não grava (teste na Tarefa 5).
5. **`per_game_keep` com 0 ou 1000 (config editada à mão):** é limitado ao intervalo de 1 a 100 ao ser usado; nunca apaga a versão recém-criada (teste na Tarefa 4).

---

### Task 1: Versão de segurança marcada

**Files:**
- Modify: `crates/core/src/backup/library.rs` (`SnapshotRef`)
- Modify: `crates/core/src/backup/snapshot.rs` (`SnapshotMeta`)
- Modify: `crates/core/src/backup/mod.rs` (nova `set_safety`, ao lado de `set_pinned`)
- Modify: `crates/core/src/restore.rs` (marca a versão de segurança)
- Modify: `src-tauri/src/commands.rs` (`SnapshotView`)
- Modify: `src/lib/types.ts`, `src/screens/Restaurar.svelte`

**Interfaces:**
- Produces:
  - `SnapshotRef.seguranca: bool` e `SnapshotMeta.seguranca: bool`, ambos com `#[serde(default)]`.
  - `pub fn set_safety(dest_root: &Path, game_id: &str, stem: &str) -> Result<(), CoreError>`, que grava `seguranca = true` no índice e no `.json`.
  - `pub fn latest_restorable(refs: &[SnapshotRef]) -> Option<&SnapshotRef>`, a mais recente com `!seguranca`, exportada em `backup`.
  - `SnapshotView.seguranca: bool`.

- [ ] **Step 1: Write the failing tests** em `backup/mod.rs` e `restore.rs`:
  - `latest_ignora_versao_de_seguranca`: duas refs, a mais nova com `seguranca: true` → `latest_restorable` devolve a mais antiga. Com todas de segurança → `None`.
  - `set_safety_grava_indice_e_json`: depois de `set_safety`, `LibraryIndex::load` e `load_meta` mostram `seguranca == true`.
  - `json_antigo_sem_seguranca_le_false`: um `.json` sem o campo é lido com `seguranca == false`.
  - Em `restore.rs`, estender o teste existente que verifica `rep.safety_snapshot`: a ref devolvida e o índice têm `seguranca == true` e `pinned == true`.
- [ ] **Step 2: Run** `cargo test -p savesgg-core seguranca` → FAIL (campo/função inexistente).
- [ ] **Step 3: Implement** os campos, `set_safety` (mesmo padrão de `set_pinned`) e `latest_restorable`. Em `apply_restore`, chamar `set_safety` logo depois de `set_pinned` e devolver `SnapshotRef { pinned: true, seguranca: true, ..r }`. `list_snapshots` preenche `SnapshotView.seguranca`. Em `Restaurar.svelte`, a versão com `seguranca` mostra o chip "versão de segurança" (com `title="Cópia do que estava no PC antes de uma restauração"`), e o chip "mais recente" vai para a primeira versão sem `seguranca`.
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(backup): marca a versão de segurança criada antes de restaurar`

### Task 2: Restaurar vários jogos

**Files:**
- Modify: `crates/core/src/restore.rs` (nova `restore_latest`)
- Modify: `src-tauri/src/commands.rs` (comando `restore_many`)
- Modify: `src/screens/Restaurar.svelte`, `src/lib/api.ts`, `src/lib/types.ts`
- Create: `src/screens/Restaurar.test.ts`

**Interfaces:**
- Consumes: `latest_restorable` (Tarefa 1); `plan_restore`, `apply_restore`, `vars_for` e `load_meta`, que já existem.
- Produces:
  - O enum serializado com `#[serde(tag = "tipo", rename_all = "snake_case")]`:
    ```rust
    pub enum LoteResultado {
        Restaurado { arquivos: usize, seguranca: Option<String> },
        PrecisaDePasta { faltando: usize },
        Erro { mensagem: String },
    }
    ```
  - `pub fn restore_latest(dest_root: &Path, game_id: &str, ctx: &PathContext, installed: &[InstalledGame], now: DateTime<Utc>) -> LoteResultado`.
  - O comando `restore_many(app: AppHandle, state, destination_id: String, game_ids: Vec<String>) -> Res<Vec<LoteItemView>>`, com `LoteItemView { game_id, game_name, resultado: LoteResultado }`.
  - O evento `restauracao-progresso`, com payload `{ feitos: usize, total: usize, jogo: String }`.
  - Na interface: `api.restoreMany(destinationId, gameIds)`.

- [ ] **Step 1: Write the failing tests** em `restore.rs`, usando a fixture dos testes de restauração que já existem:
  - `lote_restaura_a_mais_recente`: com duas versões, o arquivo restaurado tem o conteúdo da mais nova → `Restaurado { arquivos: 1, seguranca: None }`.
  - `lote_pula_versao_de_seguranca`: a mais nova está marcada como segurança → restaura a anterior.
  - `lote_so_seguranca_vira_erro`: todas as versões são de segurança → `Erro { mensagem }` contendo "só há versões de segurança".
  - `lote_sem_destino_resolvido_pede_pasta`: um template `<base>` sem jogo instalado → `PrecisaDePasta { faltando: 1 }`, sem escrever nada.
  - `lote_jogo_inexistente_vira_erro`: um `game_id` fora do índice → `Erro`.
- [ ] **Step 2: Run** `cargo test -p savesgg-core lote` → FAIL.
- [ ] **Step 3: Implement `restore_latest`**:
  1. Carrega o índice e escolhe a versão com `latest_restorable`.
  2. Monta o plano.
  3. Se algum item ficar sem `target`, devolve `PrecisaDePasta`.
  4. Senão, chama `apply_restore` sem overrides e converte qualquer `CoreError` em `Erro { mensagem: e.to_string() }`.
- [ ] **Step 4: Implement o comando `restore_many`** (só os ids conhecidos são processados):
  1. Valida que cada id está no índice; ids desconhecidos viram `Erro`, sem abortar.
  2. Segura `state.locks.lock(&destination_id)` durante todo o lote.
  3. Chama `detect_all()` uma única vez.
  4. Emite `restauracao-progresso` antes de cada jogo e roda tudo em `blocking`.
- [ ] **Step 5: Write the failing UI test** em `Restaurar.test.ts`, mockando `api`:
  - `marca_todos_e_restaura_selecionados`: com dois jogos listados, as duas caixas começam marcadas e o botão diz "Restaurar selecionados (2)". Ao desmarcar um e clicar, `restoreMany` é chamado com um id só.
  - `mostra_resumo_por_resultado`: com `restoreMany` devolvendo um `restaurado` e um `precisa_de_pasta`, a tela mostra "1 restaurado" e lista o jogo que precisa de pasta, com o botão "Escolher pasta…", que abre o fluxo atual daquele jogo (`prepararRestauracao`).
- [ ] **Step 6: Implement a interface** em `Restaurar.svelte`:
  - uma caixa de seleção por jogo e "Marcar todos / nenhum";
  - o botão primário "Restaurar selecionados (N)";
  - durante o lote, uma barra `<progress>` alimentada por `listen("restauracao-progresso")`;
  - no fim, o resumo agrupado (restaurados, precisam de você, erros).

  O fluxo de um jogo e uma versão continua como está.
- [ ] **Step 7: Run** a verificação global → PASS.
- [ ] **Step 8: Commit** `feat(restaurar): restaura vários jogos de uma vez`

### Task 3: Procurar backup em outra pasta

**Files:**
- Modify: `crates/core/src/destinations.rs` (nova `resolve_backup_folder`)
- Modify: `src-tauri/src/state.rs` (`pastas_abertas`)
- Modify: `src-tauri/src/commands.rs` (comando `open_backup_folder` e `destination_root`)
- Modify: `src/screens/Restaurar.svelte`, `src/lib/api.ts`

**Interfaces:**
- Consumes: `legado::migrar_destino` e `legado::migrar_pasta_sincronizada`, que já existem.
- Produces:
  - `pub fn resolve_backup_folder(picked: &Path) -> Option<PathBuf>`: devolve a raiz do destino, isto é, a pasta que contém `SavesGG/biblioteca.json`.
  - `AppState.pastas_abertas: Mutex<Vec<PathBuf>>`.
  - O comando `open_backup_folder(app, state) -> Res<Option<Destination>>`: devolve `None` se o usuário cancelar e `Err("Nenhum backup do SavesGG nesta pasta")` se a pasta não tiver backup. A `Destination` devolvida tem `id = "aberta:" + norm(raiz).to_lowercase()`, `label = "Pasta escolhida"` e `kind = PastaLocal`.
  - Na interface: `api.openBackupFolder()`.

- [ ] **Step 1: Write the failing tests** em `destinations.rs`:
  - `pasta_com_savesgg_dentro`: escolher `X`, que tem `X/SavesGG/biblioteca.json` → `Some(X)`.
  - `escolheu_a_propria_pasta_savesgg`: escolher `X/SavesGG` → `Some(X)`.
  - `escolheu_savesgg_backups`: escolher `X/SavesGG Backups`, que tem `SavesGG/biblioteca.json` dentro → `Some(X/SavesGG Backups)`.
  - `pasta_antiga_saveguard_migra`: escolher `X`, que tem `X/SaveGuard/biblioteca.json` → `Some(X)`, e a pasta passa a se chamar `SavesGG`.
  - `pasta_sem_backup`: uma pasta vazia → `None`.
- [ ] **Step 2: Run** `cargo test -p savesgg-core pasta_` → FAIL.
- [ ] **Step 3: Implement `resolve_backup_folder`.** Ela tenta, nesta ordem, `picked`, `picked.parent()` (quando `picked` se chama `SavesGG` ou `SaveGuard`) e `picked.join(PASTA_NA_NUVEM)`. Antes de checar cada candidato, chama `legado::migrar_destino(candidato)`.
- [ ] **Step 4: Implement o comando `open_backup_folder`**, que guarda a raiz em `pastas_abertas` sem duplicar. `destination_root` passa a aceitar os ids `aberta:*`: procura uma entrada de `pastas_abertas` com o mesmo id e exige que a pasta ainda exista.
- [ ] **Step 5: Implement a interface** em `Restaurar.svelte`:
  - O botão "Procurar backup em outra pasta…" aparece sempre, e não só quando a lista está vazia.
  - O destino devolvido entra na lista local `destinos` e já é selecionado.
  - O erro do comando aparece no `<p class="erro">`.

  O teste `abre_pasta_avulsa` em `Restaurar.test.ts` mocka `openBackupFolder` devolvendo um destino e confere que ele aparece e que `listSnapshots` é chamado com `aberta:…`.
- [ ] **Step 6: Run** a verificação global → PASS.
- [ ] **Step 7: Commit** `feat(restaurar): procura backup em qualquer pasta`

### Task 4: Fixar versões e quantas guardar

**Files:**
- Modify: `crates/core/src/config.rs` (`validar_keep`, `keep_for` limitado)
- Modify: `src-tauri/src/commands.rs` (os comandos `set_pinned_cmd`, `set_keep_versions` e `set_game_keep`, e o campo `keep` em `GameSnapshotsView`)
- Modify: `src/screens/Restaurar.svelte`, `src/screens/Configuracoes.svelte`, `src/lib/api.ts`, `src/lib/types.ts`

**Interfaces:**
- Produces:
  - `pub const KEEP_MIN: usize = 1; pub const KEEP_MAX: usize = 100;`
  - `pub fn validar_keep(n: usize) -> Result<usize, String>`, com a mensagem "Escolha entre 1 e 100 versões".
  - `keep_for` passa a devolver o valor limitado com `clamp(KEEP_MIN, KEEP_MAX)`.
  - Os comandos:
    - `set_pinned_cmd(state, destination_id, game_id, stem, pinned: bool) -> Res<()>`: valida que o `stem` está no índice e usa a trava do destino;
    - `set_keep_versions(state, n: usize) -> Res<()>`;
    - `set_game_keep(state, game_id: String, n: Option<usize>) -> Res<()>`: `None` remove a entrada de `per_game_keep`.
  - `GameSnapshotsView.keep: Option<usize>`, o valor salvo para o jogo.

- [ ] **Step 1: Write the failing tests:**
  - Em `config.rs`:
    - `keep_fora_do_intervalo_e_limitado`: `per_game_keep` com 0 → `keep_for == 1`; com 1000 → `100`;
    - `validar_keep_rejeita_0_e_101`.
  - Em `commands.rs`, nos testes que já existem:
    - `fixar_versao_inexistente_falha`: `stem` desconhecido → `Err`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** as funções e os comandos. `set_pinned_cmd` chama `savesgg_core::backup::set_pinned`.
- [ ] **Step 4: Implement a interface:**
  - Em `Restaurar.svelte`:
    - cada versão ganha o botão "Fixar" ou "Desafixar", que chama a API e recarrega a lista;
    - o cartão do jogo ganha `<select>` "Guardar:" com as opções `padrão, 3, 5, 10, 20, 50`.
  - Em `Configuracoes.svelte`, a linha "Versões guardadas por jogo" vira `<input type="number" min="1" max="100">`, salvo ao sair do campo; um erro do comando aparece no lugar do valor.
  - Teste `fixar_chama_api` em `Restaurar.test.ts`.
- [ ] **Step 5: Run** a verificação global → PASS.
- [ ] **Step 6: Commit** `feat(versoes): fixa versões e escolhe quantas guardar`

### Task 5: Jogo manual e possíveis saves descartados

**Files:**
- Modify: `crates/core/src/scan.rs` (nova `validar_nome_jogo`)
- Modify: `src-tauri/src/commands.rs` (os comandos `add_manual_game`, `remove_manual_game` e `ignored_games`)
- Modify: `src/screens/Inicio.svelte`, `src/components/GameRow.svelte`, `src/components/GameCard.svelte`, `src/screens/Configuracoes.svelte`, `src/lib/api.ts`, `src/lib/types.ts`
- Create: `src/components/AdicionarJogo.svelte`

**Interfaces:**
- Consumes: `game_id(name)`, `confirm_heuristic(id, true)` e `run_scan_blocking`, que já existem.
- Produces:
  - `pub fn validar_nome_jogo(nome: &str) -> Result<String, String>`: devolve o nome com `trim`, com 1 a 80 caracteres e sem `char::is_control`.
  - Os comandos:
    - `add_manual_game(app, state, name: String) -> Res<Option<Vec<GameView>>>`:
      1. Valida o nome.
      2. Recusa um id que já existe em `manual_games`, com a mensagem "Já existe um jogo com esse nome".
      3. Abre o seletor da pasta do save; se o usuário cancelar, devolve `None`.
      4. Abre o seletor opcional do `.exe` (filtro `exe`).
      5. Grava e refaz a varredura.
    - `remove_manual_game(state, game_id) -> Res<()>`: remove de `manual_games` a entrada cujo `game_id(name) == game_id` e mantém os backups.
    - `ignored_games(state) -> Res<Vec<IgnoredView>>`, com `IgnoredView { id: String, name: String }`. O nome vem da última varredura; se não houver, usa o próprio id.

- [ ] **Step 1: Write the failing tests:**
  - Em `scan.rs`: `nome_valido_e_aparado`, `nome_vazio_ou_espacos_recusado`, `nome_com_81_caracteres_recusado`, `nome_com_quebra_de_linha_recusado`.
  - Em `commands.rs`: `remover_manual_tira_da_config`, usando a config em memória do mesmo jeito que os testes atuais.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** a validação e os comandos.
- [ ] **Step 4: Implement a interface:**
  - `AdicionarJogo.svelte`: um formulário com nome e o botão "Escolher pasta do save e adicionar". O erro aparece no próprio formulário.
  - No Início, o botão "Adicionar jogo" abre o formulário.
  - `GameRow` e `GameCard` ganham o botão "Remover" quando `game.source === "manual"`, com `confirm("Remover … da lista? Os backups continuam guardados.")`.
  - Em Configurações, uma seção "Possíveis saves descartados" com "Trazer de volta". Se a lista estiver vazia: "Nenhum descartado".
  - Testes: `adicionar_jogo_mostra_erro_do_comando` e `descartados_trazer_de_volta`.
- [ ] **Step 5: Run** a verificação global → PASS.
- [ ] **Step 6: Commit** `feat(jogos): adiciona jogo à mão e recupera descartados`

### Task 6: Último jogado

**Files:**
- Modify: `crates/core/src/scan.rs` (`DetectedGame.last_modified`, preenchido em `build_game` e `refresh_files`)
- Modify: `crates/core/src/protection.rs` (`sort_for_display`)
- Modify: `crates/core/src/restore.rs` e todos os literais de `DetectedGame` (adicionar `last_modified: None`)
- Modify: `src-tauri/src/views.rs` (`GameView.last_played`)
- Modify: `src/components/GameRow.svelte`, `src/components/GameCard.svelte`, `src/lib/types.ts`

**Interfaces:**
- Produces:
  - `DetectedGame.last_modified: Option<DateTime<Utc>>`, a maior `modified()` entre os arquivos do save.
  - `GameView.last_played: Option<String>`, em RFC 3339.
  - A ordem de `sort_for_display` passa a ser `(priority, Reverse(last_modified), nome em minúsculas)`. `None` vem depois de qualquer data.

- [ ] **Step 1: Write the failing tests:**
  - `last_modified_e_o_arquivo_mais_novo`: com dois arquivos e datas definidas por `File::set_modified`, o campo vale a data do mais novo.
  - `ordena_por_prioridade_depois_ultimo_jogado`: com prioridades iguais, o jogo de data mais nova vem antes e o jogo sem data fica por último.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.** O laço que já soma `metadata().len()` também pega `modified()`.
- [ ] **Step 4: Implement a interface.** `GameRow` e `GameCard` mostram `Jogado {formatRelative(game.last_played)}` ao lado de "Último backup". O teste dos componentes cobre o texto.
- [ ] **Step 5: Run** a verificação global → PASS.
- [ ] **Step 6: Commit** `feat(jogos): ordena pelo último jogado`

### Task 7: Aviso quando o app da nuvem está fechado

**Files:**
- Modify: `crates/core/src/destinations.rs` (`Destination.cliente_ativo` e `cliente_ativo`)
- Modify: `src/components/DestinationPicker.svelte`, `src/screens/Configuracoes.svelte`, `src/screens/Assistente.svelte`, `src/lib/types.ts`

**Interfaces:**
- Produces:
  - `Destination.cliente_ativo: Option<bool>`: `None` fora das pastas sincronizadas.
  - `pub fn cliente_ativo(provider: SyncProvider, processos: &[String]) -> bool`: compara sem diferenciar maiúsculas com `OneDrive.exe`, `GoogleDriveFS.exe` ou `Dropbox.exe`.
  - `build_destinations` ganha o parâmetro `processos: &[String]`. `list_destinations` faz uma única leitura de processos com `sysinfo` (`ProcessesToUpdate::All`).

- [ ] **Step 1: Write the failing tests:**
  - `cliente_ativo_reconhece_processo`: `onedrive.exe` na lista → `true`; lista vazia → `false`.
  - `destino_sincronizado_recebe_cliente_ativo`: `build_destinations` com uma raiz OneDrive e `processos` vazio → `Some(false)`. Um pendrive → `None`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** o campo, a função e o parâmetro, atualizando as chamadas e os testes que já existem.
- [ ] **Step 4: Implement a interface.** Quando `cliente_ativo === false`, aparece o aviso `O {label} não está aberto: os backups ficam na pasta, mas só sobem quando ele abrir.` (cor `--aviso`) no seletor de destino, em Configurações e no passo de destinos do assistente. O teste `avisa_cliente_fechado` em `DestinationPicker.test.ts` cobre isso.
- [ ] **Step 5: Run** a verificação global → PASS.
- [ ] **Step 6: Commit** `feat(destinos): avisa quando o app da nuvem está fechado`

### Task 8: Log em arquivo

**Files:**
- Modify: `src-tauri/Cargo.toml` (`tauri-plugin-log = "2"`, `log = "0.4"`)
- Modify: `src-tauri/src/lib.rs` (plugin), `src-tauri/src/auto.rs` e outros pontos com `eprintln!`
- Modify: `src-tauri/src/commands.rs` (comando `open_logs_folder`)
- Modify: `src/screens/Sobre.svelte`, `src/lib/api.ts`

**Interfaces:**
- Produces: o comando `open_logs_folder(app) -> Res<()>`, que roda `explorer` em `app.path().app_log_dir()`, criando a pasta antes se não existir.

- [ ] **Step 1: Configure o plugin**:
  - Destinos: `Target::new(TargetKind::LogDir { file_name: Some("savesgg".into()) })` e `TargetKind::Stdout`.
  - `max_file_size(1_000_000)` e `rotation_strategy(RotationStrategy::KeepSome(5))`.
  - Nível `Info` em release e `Debug` com `cfg!(debug_assertions)`.
- [ ] **Step 2: Troque** cada `eprintln!` por `log::warn!` ou `log::error!`. Acrescente `log::info!` nestes pontos:
  - fim da varredura (quantos jogos e milissegundos);
  - cada resultado de `execute_due` (jogo, destino, resultado);
  - `restore_many` e `apply_restore` (jogo e resultado);
  - a atualização (Tarefa 9).

  Nunca registre conteúdo de arquivo nem tokens.
- [ ] **Step 3: Verify:** `grep -rn "eprintln" src-tauri/src` não acha nada. `npm run tauri dev`, depois uma varredura, e o arquivo `savesgg.log` aparece em `%LOCALAPPDATA%\com.savesgg.app\logs` com a linha da varredura.
- [ ] **Step 4: Implement** `open_logs_folder` e o botão "Abrir pasta de logs" em Sobre (teste `sobre_abre_logs` com `api` mockada).
- [ ] **Step 5: Run** a verificação global → PASS.
- [ ] **Step 6: Commit** `feat(diagnostico): grava log em arquivo e abre a pasta de logs`

### Task 9: Atualização automática e script de lançamento

**Files:**
- Modify: `src-tauri/Cargo.toml` (`tauri-plugin-updater = "2"`, `tauri-plugin-process = "2"`), `package.json` (`@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`)
- Modify: `src-tauri/tauri.conf.json`:
  - `version: "0.2.0"`;
  - `bundle.createUpdaterArtifacts: true`;
  - `plugins.updater.pubkey` e `plugins.updater.endpoints = [<endpoint>]`;
  - `plugins.updater.windows.installMode = "passive"`.
- Modify: `crates/core/Cargo.toml`, `src-tauri/Cargo.toml`, `package.json` (versão `0.2.0`)
- Modify: `src-tauri/src/lib.rs` (plugins), `src-tauri/capabilities/default.json` (`updater:default`, `process:allow-restart`)
- Create: `src/lib/atualizacao.ts`, `src/components/AvisoAtualizacao.svelte`, `src/components/AvisoAtualizacao.test.ts`
- Modify: `src/App.svelte`, `src/screens/Sobre.svelte`
- Create: `scripts/lancar.ps1`
- Modify: `.gitignore` (`*.key`), `README.md` (seção "Lançar uma versão")

**Interfaces:**
- Produces:
  - Em `atualizacao.ts`:
    - `export async function procurarAtualizacao(): Promise<Update | null>`, que encapsula o `check()` do plugin e devolve `null` em caso de erro, registrando o erro no console;
    - `export async function instalarEReiniciar(u: Update, aoProgredir: (pct: number) => void): Promise<void>`, que encapsula `downloadAndInstall` e `relaunch()`.
  - `AvisoAtualizacao.svelte`, com as props `{ atualizacao: Update }` (versão, notas, botão "Atualizar e reiniciar" e porcentagem).
  - `scripts/lancar.ps1 [-Ensaio]`.

- [ ] **Step 1: Gere a chave**: `npx tauri signer generate -w "$env:USERPROFILE\.tauri\savesgg.key" -p '' --ci`. Copie a chave pública para `tauri.conf.json`. Confira que `git status` não mostra nenhum `.key`.
- [ ] **Step 2: Write the failing UI test** em `AvisoAtualizacao.test.ts`, mockando `atualizacao.ts`:
  - `mostra_versao_e_notas`: com `{version: "0.2.1", body: "Correções"}`, a tela mostra "Versão 0.2.1 disponível" e "Correções".
  - `atualizar_chama_instalar`: ao clicar em "Atualizar e reiniciar", `instalarEReiniciar` é chamado e o botão fica desativado mostrando a porcentagem.
- [ ] **Step 3: Run** `npx vitest run AvisoAtualizacao` → FAIL.
- [ ] **Step 4: Implement:**
  - Registrar os plugins no Rust.
  - `atualizacao.ts` e o componente.
  - Em `App.svelte`, a verificação acontece só quando `!import.meta.env.DEV`: 30 s depois de montar e depois a cada 6 h (`setInterval`). Se houver versão nova, o aviso aparece no topo.
  - Em Sobre, o botão "Procurar atualização" mostra "Você está na versão mais recente" ou o aviso.
- [ ] **Step 5: Write `scripts/lancar.ps1`:**
  1. Se não estiver em `-Ensaio`, aborta quando a branch não é `main` ou a árvore tem mudanças.
  2. Lê a chave de `$env:USERPROFILE\.tauri\savesgg.key` para `$env:TAURI_SIGNING_PRIVATE_KEY`, sem imprimir, e define `TAURI_SIGNING_PRIVATE_KEY_PASSWORD=''`.
  3. Roda `npm run tauri build`.
  4. Copia `SavesGG_<versão>_x64-setup.exe` para `SavesGG_x64-setup.exe`, junto com o `.sig`.
  5. Escreve o `latest.json`:
     - `version`;
     - `notes`, vindas de `-Notas` ou da mensagem da tag;
     - `pub_date`, em ISO;
     - `platforms."windows-x86_64"`, com `signature` (o conteúdo do `.sig`) e `url` (`https://github.com/thelipesousa/savesgg-releases/releases/download/v<versão>/SavesGG_x64-setup.exe`).
  6. Gera o `SHA256SUMS.txt`.
  7. Se não estiver em `-Ensaio`, roda `gh release create v<versão> --repo thelipesousa/savesgg-releases`, enviando os quatro arquivos.
- [ ] **Step 6: Verify:**
  - A verificação global passa.
  - `powershell -File scripts/lancar.ps1 -Ensaio` gera o `latest.json` válido (`ConvertFrom-Json` funciona; a `signature` não está vazia; a `url` termina em `/v0.2.0/SavesGG_x64-setup.exe`) e não envia nada.
- [ ] **Step 7: Docs.** O README ganha a seção "Lançar uma versão": a chave e o backup dela, `lancar.ps1` e o repositório de lançamentos. O spec já descreve o resto.
- [ ] **Step 8: Commit** `feat(atualizacao): adiciona atualização automática assinada e script de lançamento`

### Fechamento (depois das tarefas)

- [ ] Revisão final da branch: `delegar` revisor ×2. Se o FreeLLMAPI estiver fora do ar, um subagente revisor no modelo mais capaz. Corrigir os itens importantes.
- [ ] Atualizar o README ("Restaurar", "Versões", "Jogo manual", "Atualização") e marcar na issue #18 o que foi feito.
- [ ] Abrir o PR para a `dev` (`feat: fecha as lacunas do MVP e adiciona atualização automática`, "Closes #18"). O merge só acontece com o ok do usuário.
- [ ] **Com o ok do usuário, à parte:**
  1. Criar o repositório público `thelipesousa/savesgg-releases` com o README de instalação.
  2. Abrir o PR `dev` → `main`.
  3. Rodar `scripts/lancar.ps1` para publicar a 0.2.0.
  4. Lembrar o usuário de guardar a cópia da chave.
