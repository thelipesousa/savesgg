# SavesGG — login no OneDrive e no Google Drive (design)

> Data: 2026-11-21 · Status: aguardando aprovação · Issue: #26 · Versão alvo: 0.3.0

## 1. Intenção e porquê

O SavesGG vai ser distribuído para primos e amigos do usuário. Hoje a nuvem só funciona se o app oficial do OneDrive ou do Google Drive estiver instalado e sincronizando a pasta. Muita gente não tem esse app, e depois de formatar ele ainda não está configurado.

A ideia é que cada pessoa **entre, dentro do SavesGG, na própria conta** do OneDrive ou do Google Drive. A partir daí, os backups sobem direto para a nuvem dela. Num PC novo, basta entrar de novo para ver e restaurar tudo.

O design do MVP já previa isso: "Login embutido via API fica para depois do MVP e deve entrar como mais um tipo de destino, sem mudar o formato do backup."

Sucesso significa:
- um amigo instala o app e clica em "Entrar com Google";
- os saves dele passam a aparecer no Drive dele, numa pasta "SavesGG Backups";
- depois de formatar, ele entra de novo e usa "Restaurar selecionados" para trazer tudo de volta.

## 2. Objetivos, não-objetivos e anti-objetivos

**Objetivos**
1. Entrar e sair de contas Microsoft (OneDrive) e Google (Drive) pelo app, com OAuth 2.0 + PKCE no navegador do sistema.
2. Conta conectada vira destino: entra no backup automático, no backup manual e na restauração, igual aos outros destinos.
3. Enviar sozinho, em segundo plano, cada versão nova. Sem internet, a versão fica guardada e sobe depois.
4. Apagar na nuvem o que a retenção apagou (manter N versões vale também lá) e reenviar o `.json` quando uma versão é fixada.
5. Restaurar da nuvem, num jogo ou em lote, baixando só o necessário.
6. Mostrar o estado de cada conta: "tudo enviado", "3 versões esperando envio" ou o erro.

**Não-objetivos (agora)**
- Dropbox, MEGA, iCloud.
- Sincronizar saves entre dois PCs ao vivo (continua só backup e restauração).
- Criptografar os backups na nuvem com senha (fica para depois).
- Contas corporativas que exigem aprovação do administrador. Elas podem não funcionar, e o app mostra o erro.

**Anti-objetivos**
- Senha ou token gravado em arquivo, no log ou no repositório.
- Pedir acesso ao OneDrive ou ao Drive **inteiro** da pessoa.
- Uma falha de envio, ou a falta de internet, impedir o backup local.
- Apagar na nuvem algo que o app não apagou de propósito (versões que ele nunca viu, outros arquivos do Drive).
- Formato fechado: na nuvem, o backup continua abrível à mão (zip + JSON).

## 3. Restrições

- **Sem servidor próprio:** o OAuth é feito todo no PC, como "aplicativo público/instalado". A Microsoft não usa client secret; o Google, para apps de desktop, dá um "secret" que ele mesmo diz não ser confidencial.
- **Cadastro dos apps feito pelo usuário,** com o guia da seção 5.8:
  - Microsoft: no portal Azure/Entra;
  - Google: no Google Cloud Console, com o consentimento **publicado em produção**, porque em "teste" o login expira em 7 dias e só aceita 100 contas listadas.
- **Ids no build:** os ids de cliente (e o "secret" do Google) entram no build por variáveis de ambiente, **fora do Git**:
  - `SAVESGG_MS_CLIENT_ID`;
  - `SAVESGG_GOOGLE_CLIENT_ID` e `SAVESGG_GOOGLE_CLIENT_SECRET`.

  O `lancar.ps1` os lê de `%USERPROFILE%\.tauri\savesgg-oauth.json`. Um build sem eles (CI, desenvolvimento) mostra "Login na nuvem indisponível nesta versão".
- **Formato:** o formato do backup e os campos atuais não mudam. A configuração só ganha campos com `serde(default)`.
- **Windows 10/11.** Os tokens ficam no Gerenciador de Credenciais (crate `keyring`, protegido pelo DPAPI do Windows).

## 4. Abordagens consideradas

| Abordagem | Prós | Contras |
|---|---|---|
| **A. Cópia local e envio** ✅ (escolhida pelo usuário) | Reaproveita backup, retenção, índice, fila e restauração, que já estão testados; funciona sem internet; o envio é um passo isolado e testável | Precisa sincronizar (enviar o que falta, apagar o que a retenção apagou) e baixar sob demanda na restauração |
| B. Gravar direto na nuvem (camada de armazenamento) | Arquitetura mais "pura" | Mexe no coração do backup e da restauração; mais risco; sem internet, precisaria de uma fila nova de qualquer forma |
| C. Só pasta sincronizada (como hoje) | Nada a fazer | Não resolve o pedido |

## 5. Como funciona (decidido)

### 5.1 Conta conectada = destino
- **Id do destino:** `nuvem:onedrive:<id da conta>` ou `nuvem:gdrive:<id da conta>`. O id é o `id` do usuário no Graph ou o `sub` do Google.
- **Tipo novo:** `DestinationKind::Nuvem(ProvedorNuvem)`.
- **Rótulo:** "OneDrive (ana@outlook.com)" ou "Google Drive (ana@gmail.com)".
- **`root`:** é a **pasta local de preparação** `%LOCALAPPDATA%\SavesGG\nuvem\<blake3(id)[..16]>`, no mesmo formato de qualquer destino (`SavesGG\biblioteca.json`, `SavesGG\<jogo>\<stem>.zip/.json`). Assim, `backup_game`, a retenção, o fixar, o `list_snapshots` e o `plan_restore` funcionam sem mudança.
- **`available`:** `true` sempre que a conta está conectada (o backup é local; o envio vem depois). `has_backup` vem do índice local, que inclui o que foi baixado da nuvem.
- **Configuração:** ganha `contas_nuvem: Vec<ContaNuvem { provedor, id, email, nome }>`, sem tokens. A conta entra também em `auto_destinations` quando o usuário marca.
- **Proteção ("já tem cópia fora do PC"):** só conta versão **já enviada** (ver 5.4). Uma versão que ainda não subiu não marca o jogo como protegido.

### 5.2 Onde fica na nuvem

O layout é o mesmo de um destino em disco, a partir de uma raiz:
- **OneDrive:** a raiz é a pasta do app (`/me/drive/special/approot`, que aparece como **Aplicativos/SavesGG**). Escopo `Files.ReadWrite.AppFolder offline_access User.Read`.
- **Google Drive:** a raiz é uma pasta **"SavesGG Backups"** na raiz do Drive, criada pelo app. Escopo `https://www.googleapis.com/auth/drive.file openid email`.
  - O app só enxerga o que ele criou. Num PC novo, com o mesmo cadastro, ele volta a enxergar essa pasta.
  - O app guarda localmente um cache de ids de pasta (caminho → id), em `nuvem\<hash>\ids.json`. Se um id sumir, o app procura a pasta pelo nome dentro da pai e, se não achar, cria de novo.
- **Dentro da raiz:** `SavesGG/biblioteca.json` e `SavesGG/<jogo>/<stem>.zip|.json`, exatamente como no disco.

### 5.3 Login (OAuth 2.0 + PKCE, loopback)
1. Em Configurações → "Contas na nuvem", o usuário clica em "Entrar com Microsoft" ou "Entrar com Google".
2. O Rust abre um `TcpListener` em `127.0.0.1:0`, numa porta livre e só local. Depois gera `state` (32 bytes aleatórios) e `code_verifier` (64 caracteres), com `code_challenge` S256.
3. O app abre a URL de autorização no **navegador padrão**, com `redirect_uri=http://127.0.0.1:<porta>` (no Microsoft, `http://localhost:<porta>`).
4. O app espera **uma** requisição `GET` por até **5 minutos**:
   - confere o `state` (se não bater, é erro e para);
   - lê o `code`;
   - responde uma página curta: "Pronto! Pode fechar esta aba e voltar ao SavesGG".
5. O app troca o `code` por tokens no endpoint de token, com o verifier, e busca quem é a conta: o Graph `/me` ou o `userinfo` do Google.
6. O app grava o **refresh token** no Gerenciador de Credenciais (serviço `SavesGG`, usuário = id do destino). O access token só fica na memória.
7. A conta entra em `contas_nuvem` e aparece como destino.
8. O app roda logo em seguida `atualizar_da_nuvem` (5.5). Num PC formatado, o destino já aparece "com backup" na tela Restaurar.

**Sair da conta:** apaga a credencial, tira a conta da configuração e de `auto_destinations` e apaga a pasta local de preparação. **Os backups na nuvem ficam lá.**

### 5.4 Enviador (sincronização local → nuvem)
- **Estado:** `nuvem\<hash>\estado.json` guarda, para cada caminho remoto (`SavesGG/<jogo>/<arquivo>`), `{ blake3, tamanho }` do que **está na nuvem**, segundo o último envio, download ou listagem.
- **Uma passada de sincronização, sob a trava do destino:**
  1. **Envia o que falta ou mudou,** em ordem estável, sempre o `.zip` antes do `.json` da mesma versão:
     - `.json` local cujo blake3 é diferente do estado: envia (versão nova, ou fixada/desafixada);
     - `.zip` local que não está no estado, ou com outro blake3: envia.
  2. **Libera espaço:** depois que o `.zip` **e** o `.json` de uma versão estão confirmados, apaga o `.zip` local. O índice e os `.json` ficam.
  3. **Propaga a retenção:** caminho no estado cujo `.json` não existe mais no local (a retenção apagou) é apagado na nuvem, `.zip` e `.json`, e sai do estado. **Só se apaga o que está no estado,** ou seja, o que o app enviou ou baixou.
  4. **Índice remoto:** envia o `biblioteca.json` local quando mudou. Ele é só uma conveniência para quem abre a pasta à mão; a listagem confia nos `.json` das versões.
- **Arquivo grande:**
  - OneDrive: envio simples até 4 MiB e sessão de upload acima disso, em pedaços de 10 MiB (múltiplos de 320 KiB).
  - Google: multipart até 5 MiB e upload "resumable" acima disso.
- **Erros:**
  - HTTP 401: renova o access token com o refresh token e tenta de novo uma vez.
  - 429/5xx/sem rede: espera 30 s, 2 min, 10 min (mesma política do backup), e a passada para; os arquivos ficam para a próxima.
  - Login revogado (`invalid_grant`): a conta fica marcada "Entre de novo" e o envio para até lá. O backup local continua.
- **Quando roda:** o motor dispara uma passada (num thread próprio, sem travar o laço) depois de cada backup automático ou manual num destino de nuvem, e a cada 10 minutos se houver algo esperando. Também roda ao abrir o app e quando o usuário clica em "Enviar agora".
- **Status:** `AutoStatusView` ganha, por conta, `pendentes` (arquivos esperando), `enviando` e `erro`. A bandeja usa o estado "copiando" enquanto envia. Configurações mostra "Tudo enviado", "N versões esperando envio" ou o erro, com "Enviar agora".

### 5.5 Restaurar da nuvem
- **Ao escolher uma conta de nuvem na tela Restaurar** (ou abrir o destino no motor depois de formatar), o comando `atualizar_da_nuvem(destination_id)`:
  1. lista `SavesGG/` e as pastas de jogo;
  2. baixa para a preparação os `.json` que faltam;
  3. registra no estado o que viu (é "da nuvem").

  O índice local passa a mostrar todas as versões, inclusive as feitas em outro PC ou antes de formatar (`LibraryIndex::load` já junta os `.json`).
- **Antes de aplicar** (`apply_restore` e `restore_many`), para destino de nuvem, `garantir_zip(destino, jogo, stem)` baixa o `.zip` para a preparação, se não estiver lá. A restauração já confere cada arquivo pelo blake3 do `.json`.
- **Nomes vindos da nuvem são validados antes de virar caminho local:**
  - jogo: só `[a-z0-9-]`, 1 a 100 caracteres;
  - versão: `AAAA-MM-DD_HH-MM-SS`, com sufixo opcional `-N`;
  - extensões: só `.zip` e `.json`.

  Qualquer outro nome é ignorado. Isso protege contra travessia de caminho.

### 5.6 Dois PCs na mesma conta
Funciona, mas sem coordenação ao vivo:
- cada PC envia as próprias versões;
- a restauração mostra as de todos;
- a retenção de um PC vale para tudo o que ele já viu na nuvem (conta junto as versões do outro PC que ele baixou).

O `biblioteca.json` remoto pode ser sobrescrito por qualquer PC, e não faz mal: a listagem usa os `.json` das versões.

### 5.7 Interface
- **Configurações → "Contas na nuvem"** (seção nova):
  - os botões "Entrar com Microsoft (OneDrive)" e "Entrar com Google (Drive)";
  - para cada conta: e-mail, estado do envio, "Enviar agora" e "Sair";
  - durante o login: "Termine o login no navegador…", com "Cancelar".
- **Destinos:** a conta conectada aparece como destino em todos os lugares (seletor do backup manual, destinos automáticos, assistente, Restaurar), com o ícone de nuvem.
- **Assistente:** no passo de destinos, os botões "Entrar com Microsoft" e "Entrar com Google" aparecem junto das pastas detectadas.
- **Build sem ids:** os botões ficam desabilitados, com "Login na nuvem indisponível nesta versão".

### 5.8 Guia de cadastro (vai para o README, feito pelo usuário)
- **Microsoft (Entra):**
  1. Registrar um app em portal.azure.com → "Registros de aplicativo". Em contas suportadas, escolher "contas pessoais e de organizações".
  2. Plataforma "Aplicativos móveis e de área de trabalho", com URI de redirecionamento `http://localhost`.
  3. Ligar "Permitir fluxos de cliente público".
  4. Permissões delegadas do Graph: `Files.ReadWrite.AppFolder`, `User.Read`, `offline_access`.
  5. Copiar o "ID do aplicativo (cliente)".
- **Google (Cloud Console):**
  1. Criar um projeto e ativar a "Google Drive API".
  2. Tela de consentimento: tipo "Externo", com nome "SavesGG", **e-mail de suporte separado (não o pessoal)** e escopos `drive.file`, `openid`, `email`.
  3. **Publicar em produção.**
  4. Credenciais → ID do cliente OAuth do tipo "App para computador"; copiar o id e o secret.
- **Os ids ficam** em `%USERPROFILE%\.tauri\savesgg-oauth.json`, no formato `{"ms_client_id": "...", "google_client_id": "...", "google_client_secret": "..."}`. Esse arquivo fica fora do repositório (`*.json` dessa pasta nunca entra no Git).

## 6. Segurança (checklist)

- **1, 2, 3 (chaves fora do Git e do cliente):**
  - os ids do OAuth entram no build por variável de ambiente, fora do repositório;
  - o "secret" do Google para desktop não é confidencial, por definição do próprio Google, mas mesmo assim fica fora do Git (o GitGuardian acusaria);
  - nada de chave de servidor: não há servidor.
- **4, 7 (acesso só ao que é seu):**
  - escopos mínimos: só a pasta do app no OneDrive e só os arquivos criados pelo app no Drive;
  - o enviador só apaga caminhos que estão no estado.
- **5 (minimização):** a configuração guarda id, e-mail e nome da conta, e nada mais.
- **6, 9, 10 (sessão e tokens):**
  - o refresh token fica no Gerenciador de Credenciais (DPAPI);
  - o access token só na memória;
  - nunca vão para o log, a interface ou um arquivo.
- **11 (limite de tentativas):** o backoff nas falhas evita martelar a API; o login tem timeout de 5 min e aceita uma requisição só.
- **12 (bots):** não se aplica, porque o login é dos próprios provedores.
- **13, 14 (validar entrada):**
  - nomes vindos da nuvem validados antes de virar caminho (5.5);
  - respostas JSON das APIs desserializadas com tipos estritos;
  - `state` do OAuth conferido.
- **15:** e-mail e nome da conta aparecem na tela só por interpolação Svelte.
- **17:** erros curtos na interface; o detalhe vai para o log, sem token.
- **19:**
  - todas as chamadas são HTTPS;
  - o loopback é `127.0.0.1` só para o redirecionamento local;
  - o listener aceita só uma conexão e fecha.
- **20 (dependências):** `keyring` (recurso `windows-native`) e `rand` com versão fixa. O `ureq` já existe e passa para o core.

## 7. O que fica para quem constrói

- **Estrutura do código:**
  - módulo `crates/core/src/nuvem/`, com `mod.rs` (o trait `Provedor`: listar, enviar, baixar, apagar), `sync.rs` (a passada do 5.4, pura e testável com um provedor falso em memória), `onedrive.rs`, `gdrive.rs` e `nomes.rs` (validação);
  - em `src-tauri/src/nuvem.rs`: OAuth, `keyring`, comandos e a ligação com o motor.
- **Testes:**
  - a sincronização, a retenção na nuvem, a retomada e os nomes maliciosos ficam testados contra o provedor falso;
  - os provedores reais são testados montando as requisições e lendo respostas gravadas;
  - um teste `#[ignore]` faz ida e volta de verdade com uma conta de teste.
- **Escolhas livres:** os textos exatos e a disposição fina da seção "Contas na nuvem", que segue o visual atual. Por ser só um acréscimo a uma tela existente, não passa pelo Stitch.
- **Ajustáveis:** os intervalos (10 min entre passadas, tamanho dos pedaços).

## 8. Passos que dependem do usuário

- Cadastrar o app na Microsoft e no Google (seção 5.8) e criar o `savesgg-oauth.json`.
- Testar o login de verdade com a própria conta, e depois com a de um amigo.
- Aprovar o lançamento da 0.3.0 (ou juntar com a 0.2.0, se ela ainda não tiver saído).
