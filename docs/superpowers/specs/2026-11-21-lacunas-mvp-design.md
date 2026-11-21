# SavesGG — lacunas do MVP e preparação para distribuir (design)

> Data: 2026-11-21 · Status: aguardando aprovação · Issue: #18 · Versão alvo: 0.2.0

## 1. Intenção e porquê

O SavesGG 0.1.0 tem o motor completo, mas várias funções do design do MVP
(`2026-11-20-saveguard-design.md`) ficaram sem tela. A mais importante é a restauração
depois de formatar o PC: hoje ela é feita um jogo e uma versão por vez. O usuário também
quer passar o instalador para primos e amigos. Para isso o app precisa se atualizar
sozinho e deixar um registro do que aconteceu, para quando der problema no PC de outra
pessoa.

Sucesso significa:
- formatar, instalar, apontar para o backup e restaurar **todos** os jogos com um clique;
- um amigo instala o app uma vez e recebe as versões novas sozinho.

## 2. Objetivos, não-objetivos e anti-objetivos

**Objetivos**
1. Restaurar vários jogos de uma vez, na versão mais recente de cada um.
2. Procurar backup em uma pasta qualquer, na tela Restaurar.
3. Fixar e desafixar versões; escolher quantas versões guardar (no geral e por jogo).
4. Adicionar jogo à mão e removê-lo; ver e recuperar os "possíveis saves" descartados.
5. Ordenar por prioridade e, dentro dela, pelo último jogado.
6. Avisar quando o app da nuvem (OneDrive, Google Drive ou Dropbox) não está aberto, porque nesse caso os backups não sobem.
7. Log em arquivo e botão para abrir a pasta de logs.
8. Atualização automática assinada, publicada num repositório público só de lançamentos.

**Não-objetivos (ficam para depois)**
- Login nas nuvens via API (próximo projeto).
- Página de apresentação na Vercel (projeto à parte, depois deste).
- Os itens pequenos das issues #3, #7 e #10. Entra só o que esta entrega precisa: marcar a versão de segurança (de #3) e o log (de #7).
- Assinatura de código do instalador (certificado pago).

**Anti-objetivos**
- A restauração em lote sobrescrever um save sem guardar antes a versão de segurança.
- A atualização instalar algo sem assinatura válida.
- O código-fonte ou a chave de assinatura ficarem públicos.
- Travar a interface durante a restauração de muitos jogos.

## 3. Restrições

- Tudo que vem da interface é validado no Rust:
  - nomes de 1 a 80 caracteres;
  - de 1 a 100 versões;
  - `game_id` e `stem` precisam existir no índice;
  - pastas só chegam pelo seletor do Windows, nunca como texto.
- O formato do backup só ganha campos opcionais (`serde(default)`), para o 0.1.0 continuar lendo o que o 0.2.0 grava e vice-versa.
- Sem servidor próprio: as atualizações vêm do GitHub Releases do repositório público.

## 4. Abordagens consideradas

**Restauração em lote**

| Abordagem | Prós | Contras |
|---|---|---|
| **Comando único `restore_many` no Rust, com progresso por evento** ✅ | Um plano e uma trava por destino; a interface só mostra o resultado; não depende da janela aberta | Comando novo |
| Laço na interface chamando `apply_restore` jogo a jogo | Nenhum código novo no Rust | Fechar a janela no meio para tudo; uma ida e volta por jogo; difícil mostrar um resumo honesto |

**Onde publicar as atualizações** (decidido pelo usuário): um repositório público só de lançamentos (`thelipesousa/savesgg-releases`). Tornar público o repositório do código foi descartado.

**Atualização**

| Abordagem | Prós | Contras |
|---|---|---|
| **`tauri-plugin-updater` com `latest.json` e assinatura minisign** ✅ | Oficial do Tauri; confere a assinatura antes de instalar; usa o instalador NSIS que já existe | Exige guardar a chave privada com cuidado |
| Só avisar "tem versão nova" e abrir a página de download | Simples | O usuário (e o amigo) precisam baixar e instalar à mão; a maioria não faria |

**Pasta escolhida na hora para restaurar**

| Abordagem | Prós | Contras |
|---|---|---|
| **Pasta lembrada só durante a sessão, no estado do app** ✅ | Não vira destino do backup automático; o caminho vem do seletor, não da interface | Some ao fechar o app |
| Salvar como destino em `custom_folders` | Reaproveita o que já existe | Uma pasta só de leitura (backup antigo) viraria destino de backup sem querer |

## 5. Como funciona (decidido)

### 5.1 Versão de segurança marcada
- `SnapshotRef` e `SnapshotMeta` ganham `seguranca: bool` (padrão `false`). A restauração grava `true` na versão que cria antes de sobrescrever (além de fixá-la, como hoje).
- A tela mostra o selo "versão de segurança". A restauração em lote **nunca** escolhe uma versão de segurança como "a mais recente".

### 5.2 Restaurar vários jogos
- **Seleção:** a tela Restaurar ganha caixas de seleção por jogo, todas marcadas por padrão, mais "marcar todos / nenhum". O botão "Restaurar selecionados (N)" restaura a versão mais recente que não seja de segurança.
- **Comando `restore_many(destination_id, game_ids)`:**
  - Para cada jogo, refaz o plano no Rust.
  - Restaura só os arquivos com destino resolvido sozinho.
  - Jogos com algum arquivo sem destino vão para a lista "precisa de você", e o usuário abre cada um pelo fluxo atual (escolher pasta).
  - Emite o evento `restauracao-progresso` com `{feitos, total, jogo}` e devolve um resumo por jogo: restaurado (n arquivos e versão de segurança, se houver), precisa de pasta ou erro (mensagem).
- **Robustez:** usa a trava do destino. Um erro num jogo não para os outros.
- **Tela durante e depois:** uma barra de progresso enquanto roda; ao final, o resumo agrupado por resultado.

### 5.3 Procurar backup em outra pasta
- **Comando `open_backup_folder()`:** abre o seletor de pasta e aceita três escolhas:
  - a pasta que contém `SavesGG\biblioteca.json`;
  - a própria pasta `SavesGG`;
  - a pasta `SavesGG Backups`.

  Antes de procurar, aplica a migração do nome antigo (`legado`).
- **Pasta válida:** é guardada em `AppState.pastas_abertas` só durante a sessão e aparece na lista de destinos da tela Restaurar como "Pasta escolhida". O id é `aberta:<caminho normalizado>`.
- **Pasta sem backup:** o app avisa "Nenhum backup do SavesGG nesta pasta".
- **Resolução dos ids:** `destination_root` passa a aceitar também os ids `aberta:*` de `pastas_abertas`.

### 5.4 Versões
- **Fixar:** comando `set_pinned_cmd(destination_id, game_id, stem, pinned)`, que valida se a versão existe no índice e usa a trava do destino. Cada versão na tela Restaurar ganha o botão "Fixar" ou "Desafixar".
- **Quantas guardar no geral:** comando `set_keep_versions(n)`, aceitando de 1 a 100. Em Configurações vira um campo numérico.
- **Quantas guardar por jogo:** comando `set_game_keep(game_id, n: Option)`, aceitando de 1 a 100, ou `None` para usar o padrão. Fica no cartão do jogo na tela Restaurar, como "Guardar: padrão / 3 / 5 / 10 / 20 / 50".
- **Limite novo:** passa a valer no próximo backup do jogo. Não apaga versões na hora.

### 5.5 Jogo manual e descartados
- **Adicionar jogo:** comando `add_manual_game(name)`, que valida o nome, abre o seletor da pasta do save e, depois, um seletor opcional do `.exe`. Grava em `manual_games` e refaz a varredura.
  - No Início, o botão "Adicionar jogo" abre um formulário pequeno com nome, "Escolher pasta do save" e "Escolher .exe (opcional)".
- **Remover jogo manual:** comando `remove_manual_game(game_id)`. A linha do jogo manual ganha o botão "Remover", que apaga só o cadastro e mantém os backups.
- **Descartados:** o comando `ignored_games()` devolve `{id, nome}` usando a última varredura. O nome vem da varredura; se não houver, mostra o id.
  - Em Configurações, uma seção "Possíveis saves descartados" lista cada um com o botão "Trazer de volta", que reaproveita `confirm_heuristic(id, true)`.

### 5.6 Último jogado
- `DetectedGame` ganha `last_modified: Option<DateTime<Utc>>`, a data de modificação mais nova entre os arquivos do save. É calculada no mesmo laço que já soma os tamanhos.
- `sort_for_display` ordena por prioridade, depois por `last_modified` (mais novo primeiro, quem não tem data vai para o fim) e por fim pelo nome.
- `GameView` ganha `last_played`, e a linha e o cartão do jogo mostram "Jogado há 2 dias".

### 5.7 App da nuvem aberto
- `Destination` ganha `cliente_ativo: Option<bool>`: preenchido só nas pastas sincronizadas, com `true` quando o processo do cliente está rodando (`OneDrive.exe`, `GoogleDriveFS.exe` ou `Dropbox.exe`, via `sysinfo`).
- Com `false`, o seletor de destino, as Configurações e o assistente mostram o aviso: "O OneDrive não está aberto: os backups ficam na pasta, mas só sobem quando ele abrir".

### 5.8 Log
- `tauri-plugin-log` grava na pasta de logs do app (`%LOCALAPPDATA%\com.savesgg.app\logs`): um arquivo de até 1 MB, mantendo os 5 mais recentes. O nível é `info` no app instalado e `debug` em desenvolvimento.
- Os `eprintln!` viram `log::warn!` ou `log::error!`. Eventos importantes vão para o log: varredura (quantos jogos, tempo), cada backup (jogo, destino, resultado), restauração, atualização e erros.
- **O log nunca grava o conteúdo dos saves nem tokens.** Grava caminhos, que ficam só no PC.
- Em Sobre, o botão "Abrir pasta de logs" chama `open_logs_folder()`, que abre o Explorer na pasta.

### 5.9 Atualização automática
- **Plugins:** `tauri-plugin-updater` e `tauri-plugin-process` (para reiniciar). O endpoint é `https://github.com/thelipesousa/savesgg-releases/releases/latest/download/latest.json`.
- **Chave:** a chave pública vai no `tauri.conf.json`. A chave privada é gerada com `tauri signer generate` e fica **fora do repositório**, em `%USERPROFILE%\.tauri\savesgg.key`. O usuário precisa guardar uma cópia, porque sem ela não dá para publicar atualizações para quem já instalou. *(Decisão minha: a chave é gerada sem senha, porque o script roda sem interação; a proteção vem de ela ficar fora do repositório e só neste PC.)*
- **Verificação:** no app instalado, 30 s depois de abrir e depois a cada 6 h, o app procura versão nova.
  - Se houver, aparece um aviso no topo: "Versão X disponível", com as notas e o botão "Atualizar e reiniciar".
  - O app baixa, confere a assinatura, instala e reinicia.
  - Em Sobre há o botão "Procurar atualização".
  - Em desenvolvimento a verificação fica desligada.
- **Lançamento:** o script `scripts/lancar.ps1`:
  1. confere se está na `main` com a árvore limpa;
  2. compila com a chave (variáveis `TAURI_SIGNING_PRIVATE_KEY*`, lidas do arquivo, nunca impressas);
  3. gera o `latest.json` (versão, notas, data, assinatura e URL do `-setup.exe`);
  4. cria a release `vX.Y.Z` em `savesgg-releases`, com o instalador, a assinatura, o `latest.json` e o `SHA256SUMS.txt`.

  No envio, o script renomeia o instalador para um nome estável (`SavesGG_x64-setup.exe`), para o link "baixar a última versão" da futura página nunca mudar.
- **Repositório público:** `thelipesousa/savesgg-releases` é criado público e vazio, só com um README curto (o que é o SavesGG e como instalar). O código continua no repositório privado.
- A versão vai para **0.2.0**.

## 6. Segurança (checklist)

- **Item 1–2 (chaves e segredos fora do Git):** a chave privada fica fora do repositório e é lida só pelo script de lançamento. O `.gitignore` ganha `*.key`.
- **Itens 7–8 e 14 (validar toda entrada no servidor, nada de campo indevido):** os comandos novos validam `game_id`, `stem`, os números e os nomes, e as pastas só chegam pelo seletor do Windows.
- **Item 15 (escapar o conteúdo do usuário na tela):** os nomes de jogos manuais aparecem sempre por interpolação do Svelte, nunca com `{@html}`.
- **Item 16 (upload):** não se aplica.
- **Item 17 (respostas enxutas):** os erros que vão para a interface continuam curtos, e o detalhe fica no log.
- **Item 19 (HTTPS):** a atualização só usa HTTPS, e o plugin recusa arquivos sem assinatura válida.
- **Item 20 (dependências):** os plugins novos entram com versão fixa e passam por `cargo audit` e `npm audit` no CI.
- **Capabilities:** entram só `updater:default` e `process:allow-restart`. O comando de abrir a pasta de logs fica no Rust, com caminho fixo.

## 7. O que fica para quem constrói

- O texto exato das mensagens e a disposição fina dos elementos novos, seguindo o visual atual (são acréscimos às telas que já existem, sem telas novas, então não passam pelo Stitch).
- Os nomes internos de funções e eventos podem mudar, desde que o comportamento acima se mantenha.
- O intervalo de 6 h entre as verificações de atualização pode ser ajustado.

## 8. Passos que dependem do usuário

- Aprovar a criação do repositório público `savesgg-releases` (o conteúdo fica público).
- Guardar uma cópia da chave privada de assinatura, por exemplo num gerenciador de senhas.
- Aprovar a primeira publicação da 0.2.0 (PR `dev` → `main` e depois `lancar.ps1`).
