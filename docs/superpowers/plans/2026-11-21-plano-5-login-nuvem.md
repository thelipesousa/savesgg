# Plano 5: login no OneDrive e no Google Drive

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pessoa entra na própria conta do OneDrive ou do Google Drive dentro do SavesGG. A conta vira um destino, os backups sobem sozinhos e é possível restaurar da nuvem depois de formatar.

**Architecture:** É a abordagem A do spec. A conta conectada é um destino cuja `root` fica numa pasta local de preparação, no formato atual. Um enviador puro (`nuvem::sync`) sincroniza essa pasta com a nuvem por meio do trait `Provedor`, que tem implementações para OneDrive e Google Drive sobre um trait `Http` (testável com respostas gravadas). O OAuth (PKCE e loopback), o `keyring`, os comandos e a ligação com o motor ficam no app Tauri.

**Tech Stack:**
- Rust: `ureq` 3 (passa a ser usado também no core), `keyring` 3 (`windows-native`), `sha2` e `base64` (PKCE), `rand` (state e verifier).
- Interface: Svelte 5 e Vitest.

**Spec:** `docs/superpowers/specs/2026-11-21-login-nuvem-design.md` (issue #26)

## Global Constraints

**Formato e configuração**
- O formato do backup não muda.
- A configuração só ganha `contas_nuvem`, com `serde(default)`. Ela **nunca** guarda token.

**Ids e caminhos fixos**
- Id do destino: `nuvem:onedrive:<id da conta>` ou `nuvem:gdrive:<id da conta>`.
- Preparação local: `%LOCALAPPDATA%\SavesGG\nuvem\<blake3(id)[..16]>`, com `estado.json` e (só no Google) `ids.json` dentro dela.
- Raiz remota: no OneDrive, `/me/drive/special/approot`; no Google, a pasta "SavesGG Backups" na raiz do Drive. Dentro dela: `SavesGG/biblioteca.json` e `SavesGG/<jogo>/<stem>.zip|.json`.
- `ProvedorNuvem` é serializado como `"onedrive"` ou `"gdrive"`. `DestinationKind::Nuvem(p)` vira `{"tipo":"nuvem","provedor":"onedrive"}`.

**Escopos OAuth**
- Microsoft: `Files.ReadWrite.AppFolder offline_access User.Read`, tenant `common`, redirect `http://localhost:<porta>`.
- Google: `https://www.googleapis.com/auth/drive.file openid email`, redirect `http://127.0.0.1:<porta>`.
- Endpoints de autorização e token:
  - Microsoft: `https://login.microsoftonline.com/common/oauth2/v2.0/authorize` e `/token`;
  - Google: `https://accounts.google.com/o/oauth2/v2/auth` e `https://oauth2.googleapis.com/token`.

**Ids do OAuth fora do Git**
- Lidos com `option_env!` no build: `SAVESGG_MS_CLIENT_ID`, `SAVESGG_GOOGLE_CLIENT_ID` e `SAVESGG_GOOGLE_CLIENT_SECRET`.
- Sem eles, o login fica indisponível e a interface mostra "Login na nuvem indisponível nesta versão".

**Tokens**
- O refresh token fica no `keyring`, com serviço `"SavesGG"` e usuário igual ao id do destino.
- O access token só fica na memória.
- Nenhum token vai para log, interface, arquivo ou mensagem de erro.

**Loopback do OAuth**
- Escuta só em `127.0.0.1`, aceita uma requisição, tem timeout de 5 minutos e confere o `state`.

**Nomes vindos da nuvem**
- Jogo: `^[a-z0-9-]{1,100}$`.
- Versão: `^\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}(-\d+)?$`.
- Extensões: só `.zip` e `.json`.
- Qualquer outro nome é ignorado.

**Envio**
- Pedaços: OneDrive faz envio simples até 4 MiB e sessão de upload acima disso, em pedaços de 10 MiB (múltiplos de 320 KiB). Google faz multipart até 5 MiB e upload "resumable" acima disso.
- Espera entre tentativas: 30 s, 2 min e 10 min. Num 401, renova o token e tenta de novo uma vez.
- Nova passada a cada 10 minutos quando há algo esperando.

**Herdado do plano 4**
- Validação no Rust.
- Comando novo registrado em quatro lugares: `commands.rs`, `lib.rs`, `build.rs` e capabilities.
- Interpolação Svelte, sem `{@html}`.
- Verificação global: fmt, clippy `-D warnings`, `cargo test`, `vitest` e `svelte-check`.
- Commits convencionais terminando com `Refs #26` e `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

**Versão**
- 0.3.0.

## Review Focus

1. **Internet cai no meio do envio de um zip grande:** a passada para sem marcar nada como enviado e a próxima recomeça sem duplicar nem corromper. Teste `sync_falha_no_meio_retoma` (Tarefa 2).
2. **Arquivos remotos com nomes estranhos** (`../../x.zip`, `Jogo Com Espaço/`, `a.exe`): são ignorados no download, nada é gravado fora da preparação. Teste `pull_ignora_nomes_invalidos` (Tarefa 3).
3. **Na nuvem há arquivos que o app nunca enviou nem baixou:** a retenção nunca os apaga. Teste `sync_nao_apaga_o_que_nao_esta_no_estado` (Tarefa 2).
4. **Login revogado pela pessoa no site da Microsoft ou do Google** (`invalid_grant`): a conta fica "Entre de novo", o envio para e o backup local continua. Teste `token_revogado_vira_entrar_de_novo` (Tarefa 6) e `sync_erro_de_login_nao_apaga_nada` (Tarefa 2).
5. **Usuário fecha a aba do navegador sem terminar o login, ou outra página chama o loopback com `state` errado:** o login falha com uma mensagem clara em até 5 minutos e não grava nada. Testes `loopback_state_errado_falha` e `loopback_timeout` (Tarefa 6).

---

### Task 1: Conta de nuvem como destino

**Files:**
- Create: `crates/core/src/nuvem/mod.rs`, `crates/core/src/nuvem/nomes.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod nuvem;`), `crates/core/src/config.rs` (`contas_nuvem`), `crates/core/src/destinations.rs` (`DestinationKind::Nuvem`, `destinos_nuvem`, `list_destinations`, `counts_as_off_pc`)

**Interfaces:**
- Produces:
  - Os tipos de provedor e conta:
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ProvedorNuvem { Onedrive, Gdrive }
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ContaNuvem { pub provedor: ProvedorNuvem, pub id: String, pub email: String, pub nome: String }
    impl ContaNuvem { pub fn destino_id(&self) -> String }       // "nuvem:onedrive:<id>"
    pub fn pasta_preparacao(base: &Path, destino_id: &str) -> PathBuf   // base.join("nuvem").join(blake3[..16])
    ```
  - Em `nomes.rs`: `pub fn jogo_valido(s: &str) -> bool`, `pub fn versao_valida(stem: &str) -> bool`, `pub fn arquivo_remoto_valido(nome: &str) -> Option<(String /*stem*/, &'static str /*"zip"|"json"*/)>`.
  - `Config.contas_nuvem: Vec<ContaNuvem>`.
  - `pub fn destinos_nuvem(contas: &[ContaNuvem], base: &Path) -> Vec<Destination>`: rótulo `"OneDrive (<email>)"` ou `"Google Drive (<email>)"`, `available: true`, `has_backup` lido do índice da preparação e `cliente_ativo: None`.
  - `list_destinations` passa a acrescentar `destinos_nuvem(&cfg.contas_nuvem, &dirs::data_local_dir()/"SavesGG")`.
  - `counts_as_off_pc` devolve `true` para `Nuvem`.

- [ ] **Step 1: Write the failing tests:**
  - `nomes.rs`:
    - `jogo_valido_aceita_slug`: aceita `"elden-ring"`; recusa `"../x"`, `"Jogo X"`, `""` e 101 caracteres.
    - `versao_valida_formato`: aceita `"2026-11-20_21-40-05"` e `"2026-11-20_21-40-05-2"`; recusa `"2026-11-20"` e `"..\\a"`.
    - `arquivo_remoto_so_zip_e_json`: `"2026-11-20_21-40-05.zip"` dá `Some(("2026-11-20_21-40-05","zip"))`; `"a.exe"` e `"../2026-11-20_21-40-05.json"` dão `None`.
  - `destinations.rs`:
    - `conta_nuvem_vira_destino`: com uma conta OneDrive (`email "ana@outlook.com"`), o id é `nuvem:onedrive:abc`, o rótulo `"OneDrive (ana@outlook.com)"`, `kind == Nuvem(Onedrive)`, a `root` igual a `pasta_preparacao(base, id)` e `available` é verdadeiro.
    - `json_do_destino_nuvem`: `serde_json::to_value(kind)` dá `{"tipo":"nuvem","provedor":"onedrive"}`.
  - `config.rs`: `config_antiga_sem_contas_nuvem` deixa a lista vazia.
- [ ] **Step 2: Run** `cargo test -p savesgg-core -- nomes conta_nuvem json_do_destino config_antiga_sem` → FAIL (os tipos não existem).
- [ ] **Step 3: Implement** os tipos e as funções acima. Atualize os `match` de `DestinationKind` no app (`views.rs`, `commands.rs`) para compilar; o ícone é decisão da Tarefa 8.
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(nuvem): adiciona conta de nuvem como destino`

### Task 2: Enviador (`nuvem::sync`) com provedor falso

**Files:**
- Create: `crates/core/src/nuvem/sync.rs`, `crates/core/src/nuvem/falso.rs` (`#[cfg(any(test, feature = "teste"))]`)
- Modify: `crates/core/src/nuvem/mod.rs`

**Interfaces:**
- Consumes: `backup_root`, `LibraryIndex` e `nomes`.
- Produces:
  - O trait do provedor, com caminhos relativos à raiz remota, como `"SavesGG/elden-ring/2026-11-20_21-40-05.zip"`:
    ```rust
    pub trait Provedor {
        fn listar(&self, pasta: &str) -> Result<Vec<ItemRemoto>, ErroNuvem>;
        fn enviar(&self, caminho: &str, origem: &Path) -> Result<(), ErroNuvem>;
        fn baixar(&self, caminho: &str, destino: &Path) -> Result<(), ErroNuvem>;
        fn apagar(&self, caminho: &str) -> Result<(), ErroNuvem>;
    }
    pub struct ItemRemoto { pub nome: String, pub pasta: bool, pub tamanho: u64 }
    #[derive(Debug, thiserror::Error)]
    pub enum ErroNuvem { #[error("sem conexão")] Rede(String), #[error("entre de novo na conta")] Login,
                         #[error("a nuvem recusou: {0}")] Servidor(String), #[error("{0}")] Local(String) }
    ```
  - O estado da sincronização e o resumo da passada:
    ```rust
    #[derive(Serialize, Deserialize, Default)]
    pub struct EstadoNuvem { pub arquivos: BTreeMap<String, ArquivoRemoto> }   // estado.json
    #[derive(Serialize, Deserialize, Clone, PartialEq)] pub struct ArquivoRemoto { pub blake3: String, pub tamanho: u64 }
    pub struct ResumoSync { pub enviados: usize, pub apagados: usize, pub pendentes: usize }
    pub fn sincronizar(prep: &Path, prov: &dyn Provedor) -> Result<ResumoSync, ErroNuvem>;
    pub fn pendentes(prep: &Path) -> usize;   // arquivos locais que diferem do estado
    ```
  - `falso.rs`: `pub struct ProvedorFalso` (um `Mutex<BTreeMap<String, Vec<u8>>>` e `falhar_apos: Option<usize>` para simular queda), que implementa `Provedor`.

- [ ] **Step 1: Write the failing tests** em `sync.rs`, com `backup_game` numa preparação temporária e um `ProvedorFalso`:
  - `sync_envia_versao_e_libera_zip`: depois de um backup e de `sincronizar`, a nuvem falsa tem o `.zip`, o `.json` e o `biblioteca.json`; o `.zip` local sumiu, o `.json` ficou, `pendentes(prep) == 0` e `enviados == 3`.
  - `sync_sem_mudanca_nao_reenvia`: a segunda passada dá `enviados == 0`.
  - `sync_versao_fixada_reenvia_json`: com `set_pinned` numa versão já enviada, a passada envia só aquele `.json`.
  - `sync_retencao_apaga_na_nuvem`: com `keep = 1`, dois backups e passadas entre eles, a nuvem fica só com a versão nova e `apagados == 2`.
  - `sync_nao_apaga_o_que_nao_esta_no_estado`: um arquivo posto direto na nuvem falsa (`SavesGG/outro/2026-01-01_00-00-00.zip`) continua lá depois de várias passadas.
  - `sync_falha_no_meio_retoma`: com `falhar_apos: Some(1)`, a primeira passada dá `Err(Rede)`, nada de zip local é apagado e o estado não marca o que falhou. Depois de tirar a falha, a segunda passada termina com a nuvem completa.
  - `sync_erro_de_login_nao_apaga_nada`: um provedor que sempre devolve `Login` faz a passada dar `Err(Login)`, com o zip local e o estado intactos.
- [ ] **Step 2: Run** `cargo test -p savesgg-core sync_` → FAIL.
- [ ] **Step 3: Implement `sincronizar`** na ordem do spec 5.4:
  1. Carrega `estado.json` (vazio se não existir).
  2. Envia, por versão, o `.zip` antes do `.json`, e por último o `biblioteca.json`.
  3. Grava o estado de forma atômica (`backup_write_atomic`) **depois de cada arquivo enviado**.
  4. Apaga o `.zip` local quando o `.zip` e o `.json` da versão estão no estado com o blake3 local.
  5. Apaga na nuvem os caminhos do estado cujo `.json` local não existe mais.
  6. No primeiro `Err`, devolve o erro (o que já foi marcado fica marcado).
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(nuvem): adiciona o enviador local → nuvem`

### Task 3: Baixar da nuvem (listar versões, baixar zip, proteção)

**Files:**
- Modify: `crates/core/src/nuvem/sync.rs`

**Interfaces:**
- Consumes: `Provedor`, `EstadoNuvem` e `nomes` (Tarefas 1 e 2).
- Produces:
  - `pub fn atualizar_da_nuvem(prep: &Path, prov: &dyn Provedor) -> Result<usize /*jsons baixados*/, ErroNuvem>`: lista `SavesGG/` e cada pasta de jogo válida, baixa os `.json` válidos que faltam e registra no estado tudo o que viu (os `.zip` também, com o tamanho, e o blake3 `""` quando ainda não foram baixados).
  - `pub fn garantir_zip(prep: &Path, prov: &dyn Provedor, jogo: &str, stem: &str) -> Result<(), ErroNuvem>`: baixa `SavesGG/<jogo>/<stem>.zip` se não existir localmente.
  - `pub fn ultimas_enviadas(prep: &Path) -> BTreeMap<String /*jogo*/, DateTime<Utc>>`: o `created` da versão mais recente de cada jogo cujo `.zip` **e** `.json` estão no estado.

- [ ] **Step 1: Write the failing tests:**
  - `pull_traz_versoes_de_outro_pc`: uma nuvem falsa com uma versão vinda de outra preparação faz `atualizar_da_nuvem` devolver 1; `LibraryIndex::load(prep)` passa a listar a versão e a segunda chamada devolve 0.
  - `pull_ignora_nomes_invalidos`: na nuvem estão `SavesGG/../x.json`, `SavesGG/Jogo X/2026-11-20_21-40-05.json` e `SavesGG/jogo/a.exe`. Nada disso é baixado e nenhum arquivo aparece fora de `prep`.
  - `garantir_zip_baixa_so_quando_falta`: a primeira chamada baixa; a segunda não chama `baixar` (o contador do falso não muda).
  - `ultimas_enviadas_ignora_nao_enviadas`: uma versão local sem passada não aparece; depois de `sincronizar`, aparece com o `created` dela.
- [ ] **Step 2: Run** `cargo test -p savesgg-core -- pull_ garantir_zip ultimas_enviadas` → FAIL.
- [ ] **Step 3: Implement** as três funções. `ultimas_enviadas` lê os `.json` locais e cruza com o estado.
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(nuvem): restaura versões da nuvem baixando sob demanda`

### Task 4: HTTP e o provedor OneDrive

**Files:**
- Create: `crates/core/src/nuvem/http.rs`, `crates/core/src/nuvem/onedrive.rs`, `crates/core/tests/dados/onedrive/*.json` (respostas gravadas)
- Modify: `crates/core/Cargo.toml` (`ureq = "3.5.2"`)

**Interfaces:**
- Produces:
  - Em `http.rs`:
    ```rust
    pub struct Pedido { pub metodo: &'static str, pub url: String, pub cabecalhos: Vec<(String, String)>, pub corpo: Vec<u8> }
    pub struct Resposta { pub status: u16, pub cabecalhos: Vec<(String, String)>, pub corpo: Vec<u8> }
    pub trait Http: Send + Sync { fn enviar(&self, p: Pedido) -> Result<Resposta, ErroNuvem>; }
    pub struct HttpUreq;               // impl Http com ureq; erro de rede → ErroNuvem::Rede
    pub trait Token: Send + Sync { fn atual(&self) -> Result<String, ErroNuvem>; fn renovar(&self) -> Result<String, ErroNuvem>; }
    ```
  - Em `onedrive.rs`: `pub struct OneDrive<H: Http, T: Token> { http: H, token: T }`, que implementa `Provedor`.
    - Base: `https://graph.microsoft.com/v1.0/me/drive/special/approot:/<caminho>:`.
    - `listar`: `…:/children`, seguindo `@odata.nextLink`.
    - `enviar`: até 4 MiB, `PUT …:/content`; acima disso, `POST …:/createUploadSession` e depois `PUT` em pedaços de 10 MiB com `Content-Range`.
    - `baixar`: `GET …:/content`, seguindo o 302.
    - `apagar`: `DELETE`, aceitando 404 como sucesso.
    - 401: chama `token.renovar()` e repete uma vez. 401 de novo vira `Login`. 429/5xx vira `Servidor`.

- [ ] **Step 1: Write the failing tests**, com um `HttpGravado` que confere o pedido e devolve respostas fixas:
  - `onedrive_lista_com_paginacao`: duas páginas devolvem 3 itens, com pastas marcadas.
  - `onedrive_envio_pequeno_usa_put`: um arquivo de 10 bytes gera um único `PUT` com a URL `…/approot:/SavesGG/j/a.zip:/content` e o cabeçalho `Authorization: Bearer t1`.
  - `onedrive_envio_grande_usa_sessao`: um arquivo de 12 MiB gera `createUploadSession` e depois 2 `PUT`s com `Content-Range: bytes 0-10485759/12582912` e `bytes 10485760-12582911/12582912`.
  - `onedrive_401_renova_token`: a primeira resposta 401 leva a uma nova tentativa com `Bearer t2`.
  - `onedrive_apagar_404_ok`.
  - `onedrive_caminho_e_codificado`: `"SavesGG/jogo/2026-11-20_21-40-05.zip"` não leva `..` e os espaços viram `%20`. Mesmo assim os nomes já vêm validados.
- [ ] **Step 2: Run** `cargo test -p savesgg-core onedrive_` → FAIL.
- [ ] **Step 3: Implement** `http.rs` e `onedrive.rs`. `serde` com tipos estritos (`value`, `@odata.nextLink`, `uploadUrl`).
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(nuvem): adiciona o provedor OneDrive (Microsoft Graph)`

### Task 5: Provedor Google Drive

**Files:**
- Create: `crates/core/src/nuvem/gdrive.rs`, `crates/core/tests/dados/gdrive/*.json`

**Interfaces:**
- Consumes: `Http`, `Token` e `Provedor` (Tarefas 2 e 4).
- Produces: `pub struct GoogleDrive<H: Http, T: Token> { http: H, token: T, ids: PathBuf /*ids.json*/ }`, que implementa `Provedor`.
  - **Resolver pastas:** para cada segmento do caminho a partir de "SavesGG Backups", usa o cache `ids.json`. Sem cache, faz `files.list` com `q = "name='<seg>' and '<pai>' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false"`. Se não achar, cria a pasta (`files.create` com o mimeType de pasta) e guarda no cache.
  - **Raiz:** `name='SavesGG Backups' and 'root' in parents`.
  - **`enviar`:** se o arquivo já existe (mesmo nome na pasta), usa `PATCH` (substitui o conteúdo). Senão, multipart até 5 MiB (`uploadType=multipart`) ou resumable acima disso (`uploadType=resumable`, depois `PUT` no `Location`).
  - **`baixar`:** `files/<id>?alt=media`.
  - **`apagar`:** `DELETE files/<id>`, aceitando 404.
  - **Erros:** 401, 429 e 5xx como no OneDrive. Um id de pasta do cache que deu 404 é tirado do cache e resolvido de novo.

- [ ] **Step 1: Write the failing tests**:
  - `gdrive_cria_pastas_que_faltam_e_guarda_ids`: o primeiro envio cria "SavesGG Backups", "SavesGG" e "jogo" (3 `files.create`). O segundo envio não lista nem cria pastas.
  - `gdrive_envio_existente_usa_patch`.
  - `gdrive_envio_grande_resumable`: um arquivo de 6 MiB faz `POST …uploadType=resumable` e depois um `PUT` no `Location`.
  - `gdrive_id_sumido_resolve_de_novo`: um 404 numa pasta do cache leva a nova busca, nova criação e sucesso.
  - `gdrive_listar_usa_query_da_pasta`: a URL contém `'<id>' in parents` e `trashed=false`, e segue o `nextPageToken`.
- [ ] **Step 2: Run** `cargo test -p savesgg-core gdrive_` → FAIL.
- [ ] **Step 3: Implement** como acima. Os valores na `q` são escapados (`'` vira `\'`).
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(nuvem): adiciona o provedor Google Drive`

### Task 6: OAuth (PKCE, loopback) e tokens no Gerenciador de Credenciais

**Files:**
- Create: `src-tauri/src/oauth.rs`
- Modify: `src-tauri/Cargo.toml` (`keyring = { version = "3.6.3", features = ["windows-native"] }`, `sha2 = "0.10.9"`, `base64 = "0.22.1"`, `rand = "0.9.2"`)

**Interfaces:**
- Consumes: `ProvedorNuvem`, `ContaNuvem`, `Token` e `ErroNuvem`.
- Produces:
  - A configuração do cliente:
    ```rust
    pub struct ClienteOAuth { pub provedor: ProvedorNuvem, pub client_id: &'static str, pub client_secret: Option<&'static str> }
    pub fn cliente(p: ProvedorNuvem) -> Option<ClienteOAuth>;   // None se o option_env! faltar
    ```
  - PKCE e montagem da URL:
    ```rust
    pub fn pkce() -> (String /*verifier 64*/, String /*challenge S256 base64url sem padding*/);
    pub fn url_autorizacao(c: &ClienteOAuth, porta: u16, state: &str, challenge: &str) -> String;
    ```
  - Loopback: `pub fn esperar_codigo(l: TcpListener, state: &str, prazo: Duration) -> Result<String, String>`. Lê uma requisição, extrai `code` e `state` da query e responde com uma página HTML curta. Os erros possíveis são "o login foi cancelado ou demorou demais" e "resposta do login inválida".
  - Login: `pub fn entrar(p: ProvedorNuvem, abrir: impl Fn(&str)) -> Result<(ContaNuvem, String /*refresh*/), String>`. Faz o fluxo completo: troca o código, pega a conta (Graph `/me` → `id`, `userPrincipalName`/`mail`, `displayName`; Google `userinfo` → `sub`, `email`, `name`) e grava no `keyring`.
  - `pub struct TokenKeyring { destino_id: String, cliente: ClienteOAuth, cache: Mutex<Option<String>> }`, que implementa `Token`:
    - `atual` usa o cache ou renova;
    - `renovar` usa o refresh do `keyring` no endpoint de token;
    - `invalid_grant` vira `ErroNuvem::Login`;
    - se vier um refresh novo, ele é regravado.
  - `pub fn sair(destino_id: &str)` apaga a credencial.

- [ ] **Step 1: Write the failing tests**:
  - `pkce_challenge_confere`: para o verifier `"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"`, o challenge é `"E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"` (exemplo da RFC 7636).
  - `url_microsoft_tem_escopos_e_pkce`: a URL contém `client_id=`, `response_type=code`, `redirect_uri=http%3A%2F%2Flocalhost%3A<porta>`, o escopo codificado com `Files.ReadWrite.AppFolder`, `code_challenge_method=S256` e `state=`.
  - `url_google_usa_127`: `redirect_uri=http%3A%2F%2F127.0.0.1%3A<porta>`, `access_type=offline` e `prompt=consent`.
  - `loopback_devolve_codigo`: uma thread faz `GET /?code=abc&state=s1` e `esperar_codigo(..,"s1",..)` devolve `Ok("abc")`.
  - `loopback_state_errado_falha`: `state=outro` dá `Err` e nada é gravado.
  - `loopback_timeout`: sem requisição, com prazo de 200 ms, dá `Err` com "demorou demais".
  - `token_revogado_vira_entrar_de_novo`: com uma resposta gravada `{"error":"invalid_grant"}` (via `Http` injetado num construtor de teste), `renovar()` devolve `Err(ErroNuvem::Login)`.
- [ ] **Step 2: Run** `cargo test -p savesgg -- pkce url_ loopback token_revogado` → FAIL.
- [ ] **Step 3: Implement.** O `TcpListener` faz bind em `127.0.0.1:0`. Use `set_nonblocking` com laço até o prazo, ou um `accept` com timeout via thread, aceitando uma conexão. O navegador abre com `std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url])`. Os erros devolvidos não citam token.
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(nuvem): adiciona login OAuth com PKCE e tokens no Gerenciador de Credenciais`

### Task 7: Comandos, motor e restauração

**Files:**
- Create: `src-tauri/src/nuvem.rs`
- Modify: `src-tauri/src/commands.rs` (`history`, `apply_restore`, `restore_many`), `src-tauri/src/auto.rs` (disparar o envio, `AutoStatusView`), `src-tauri/src/lib.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes: as Tarefas 1 a 6.
- Produces:
  - `pub fn provedor_de(conta: &ContaNuvem) -> Box<dyn Provedor>`, que monta `OneDrive` ou `GoogleDrive` com `HttpUreq` e `TokenKeyring`.
  - Os comandos:
    - `entrar_nuvem(app, state, provedor: ProvedorNuvem) -> Res<Destination>`: chama `oauth::entrar`, grava a conta e roda `atualizar_da_nuvem`.
    - `sair_nuvem(state, destination_id) -> Res<()>`.
    - `enviar_agora(app, state, destination_id) -> Res<()>`: agenda uma passada.
    - `atualizar_da_nuvem_cmd(state, destination_id) -> Res<usize>`.
    - `login_nuvem_disponivel() -> NuvemDisponivel { onedrive: bool, gdrive: bool }`.
  - `AutoStatusView.nuvem: Vec<StatusNuvem { destination_id, pendentes: usize, enviando: bool, erro: Option<String>, precisa_entrar: bool }>`.
  - **Integração:**
    - `history()`: para destinos `Nuvem`, usa `ultimas_enviadas(prep)` no lugar do índice inteiro.
    - `apply_restore` e `restore_many`: se o destino for `Nuvem`, chamam `garantir_zip` antes de restaurar cada jogo.
    - **Motor:** um `EnviadorNuvem` com um thread por conta, acordado por um canal. Recebe um sinal ao abrir o app, depois de cada backup num destino `Nuvem`, depois de `enviar_agora` e a cada 10 min quando `pendentes > 0`. Usa a trava do destino durante a passada, aplica o backoff de 30 s / 2 min / 10 min e marca `precisa_entrar` em `ErroNuvem::Login`.

- [ ] **Step 1: Write the failing tests:**
  - `history_nuvem_so_conta_enviadas`: um destino `Nuvem` com uma versão local não enviada tem `last_backup == None`; depois de marcar o estado como enviado, tem a data.
  - `status_nuvem_marca_precisa_entrar`: a função pura `aplicar_resultado(&mut StatusNuvem, Err(ErroNuvem::Login))` liga `precisa_entrar` e deixa `erro == Some("entre de novo na conta")`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** os comandos, o `EnviadorNuvem` e a integração. Um destino `Nuvem` sem conta no `keyring` aparece com `precisa_entrar`.
- [ ] **Step 4: Verify:**
  - verificação global → PASS;
  - com ids de teste definidos, `npm run tauri dev` → Configurações → Entrar com Microsoft → login real → a conta aparece. O primeiro backup manual nela sobe para `Aplicativos/SavesGG` (conferir no OneDrive web). Essa conferência depende do usuário ter feito o cadastro (spec 5.8); sem ele, registre no ledger "teste real pendente".
- [ ] **Step 5: Commit** `feat(nuvem): liga as contas ao motor, aos comandos e à restauração`

### Task 8: Interface das contas na nuvem

**Files:**
- Create: `src/components/ContasNuvem.svelte`, `src/components/ContasNuvem.test.ts`
- Modify: `src/screens/Configuracoes.svelte`, `src/screens/Assistente.svelte`, `src/screens/Restaurar.svelte` (chama `atualizarDaNuvem` ao escolher um destino `nuvem`), `src/components/DestinationPicker.svelte` e `Configuracoes.svelte` (ícone ☁️ para `nuvem`), `src/lib/api.ts`, `src/lib/types.ts`

**Interfaces:**
- Consumes: os comandos da Tarefa 7.
- Produces: `ContasNuvem.svelte`, com as props `{ destinos: Destination[]; status: StatusNuvem[]; onmudou: () => void }`.

- [ ] **Step 1: Write the failing tests** em `ContasNuvem.test.ts`:
  - `botoes_desabilitados_sem_ids`: com `login_nuvem_disponivel` devolvendo `{onedrive:false, gdrive:false}`, os dois botões ficam desabilitados e aparece "Login na nuvem indisponível nesta versão".
  - `entrar_mostra_aguardando_e_chama_api`: clicar em "Entrar com Microsoft (OneDrive)" chama `entrar_nuvem` com `{provedor:"onedrive"}` e mostra "Termine o login no navegador…".
  - `conta_mostra_estado`: com `pendentes: 3`, aparece "3 versões esperando envio"; com `precisa_entrar: true`, aparece "Entre de novo" e o botão "Entrar de novo".
  - `sair_pede_confirmacao`: `confirm` com o texto "Sair de ana@outlook.com? Os backups na nuvem continuam lá." e então `sair_nuvem` é chamado.
  - Em `Restaurar.test.ts`: `restaurar_nuvem_atualiza_antes_de_listar` (escolher um destino `nuvem` chama `atualizar_da_nuvem_cmd` antes de `list_snapshots`).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.** A seção "Contas na nuvem" vai em Configurações e os botões de entrar no passo de destinos do assistente. Os erros de `entrar_nuvem` aparecem em `role="alert"`.
- [ ] **Step 4: Run** a verificação global → PASS.
- [ ] **Step 5: Commit** `feat(interface): adiciona as contas na nuvem`

### Task 9: Build, guia de cadastro e versão

**Files:**
- Modify: `scripts/lancar.ps1` (lê `%USERPROFILE%\.tauri\savesgg-oauth.json` e exporta as três variáveis sem imprimir nada), `README.md` (seção "Login na nuvem": o guia do spec 5.8), `src-tauri/tauri.conf.json`, `Cargo.toml` e `package.json` (versão `0.3.0`), `.gitignore` (`savesgg-oauth.json`)

- [ ] **Step 1:** Em `lancar.ps1`, se o arquivo existir, defina `$env:SAVESGG_MS_CLIENT_ID`, `$env:SAVESGG_GOOGLE_CLIENT_ID` e `$env:SAVESGG_GOOGLE_CLIENT_SECRET` antes do build. Se não existir, avise "Login na nuvem ficará desligado nesta versão" e continue.
- [ ] **Step 2: Verify:**
  - `powershell -File scripts/lancar.ps1 -Ensaio` sem o arquivo mostra o aviso e termina.
  - `git grep -n "client_secret\|SAVESGG_GOOGLE_CLIENT_SECRET=" -- . ':!docs'` não acha nenhum valor.
- [ ] **Step 3:** O README ganha o guia de cadastro (Microsoft e Google, com "publicar em produção" e "e-mail separado") e explica onde fica o `savesgg-oauth.json`.
- [ ] **Step 4:** Versão para 0.3.0 e verificação global → PASS.
- [ ] **Step 5: Commit** `build(nuvem): lê os ids do OAuth no lançamento e documenta o cadastro`

### Fechamento

- [ ] Revisão final da branch: `delegar` revisor ×2, ou um subagente revisor no modelo mais capaz se o FreeLLMAPI estiver fora do ar. Corrigir os itens Critical e Important com teste RED → GREEN.
- [ ] Abrir o PR para a `dev` com "Closes #26". O merge só acontece com o ok do usuário.
- [ ] Pendente do usuário: fazer o cadastro (spec 5.8), testar o login real com a própria conta e com a de um amigo, e aprovar o lançamento.
