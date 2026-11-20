# SaveGuard — design (MVP)

> Data: 2026-11-20 · Status: aprovado · Nome "SaveGuard" é provisório.

## 1. Intenção e porquê

O usuário vai formatar o PC e não quer perder nenhum save de jogo — de loja (Steam, Epic, GOG, Xbox) ou não (emuladores, jogos sem loja, versões não oficiais). Depois da formatação, quer que os saves continuem protegidos sozinhos, sem ter que lembrar.

O app varre o PC, acha os saves, mostra **o que corre risco** (o que não tem nenhuma cópia fora do PC vai pro topo), faz backup contínuo quando você joga e restaura tudo no lugar certo num PC novo.

Sucesso = formatar o PC, instalar o app, apontar pro backup e ter todos os saves de volta, cada um na pasta certa.

## 2. Objetivos, não-objetivos e anti-objetivos

**Objetivos (MVP)**
1. Varredura do PC: detectar jogos e onde guardam saves.
2. Prioridade por risco: Alta (nenhuma cópia fora do PC), Média (nuvem parcial/incerta), Baixa (já protegido).
3. Reconhecer o que já está protegido: nuvem da loja, pasta sincronizada pelo OneDrive/Google Drive/Dropbox, backup anterior do próprio app.
4. Backup contínuo: copia ao fechar o jogo + vigia a pasta do save.
5. Versões: guarda as últimas N cópias por jogo.
6. Destinos: nuvem (OneDrive, Google Drive, Dropbox), pasta local, pendrive, HD externo.
7. Restaurar num PC novo, cada save no caminho certo.
8. 5 temas alternáveis na hora (seção 6).

**Não-objetivos (fora do MVP)**
- Linux, macOS, celular.
- Editar saves, trapaças, compartilhar saves com outras pessoas.
- Sincronizar saves entre dois PCs ao vivo (só backup e restauração).

**Anti-objetivos (seria fracasso mesmo funcionando)**
- Corromper ou sobrescrever um save sem perguntar. Restaurar sempre faz backup do save atual antes.
- Apagar ou mover os arquivos originais do usuário. O app só lê e copia.
- Pesar no PC enquanto você joga (queda de FPS, disco ou CPU altos).
- Formato de backup fechado. O backup precisa ser abrível à mão (zip + JSON legível), mesmo sem o app.

## 3. Restrições

- Windows 10/11 (64 bits).
- Roda o tempo todo na bandeja; precisa ser leve (meta: < 60 MB de RAM ocioso, CPU ~0% enquanto ninguém joga).
- Sem servidor próprio: tudo roda no PC do usuário.
- Interface em português do Brasil.

## 4. Abordagens consideradas

| Abordagem | Prós | Contras |
|---|---|---|
| **Tauri 2 (Rust + interface HTML)** ✅ escolhida | ~10 MB, pouca RAM; temas são CSS; reaproveita o HTML do Stitch; motor em Rust; pode usar código/manifesto do Ludusavi (Rust) | Usa o WebView2 do Windows (já vem no Win 10/11) |
| Rust puro (Slint/Iced) | O mais leve | 5 temas caprichados viram muito trabalho; não aproveita o Stitch |
| Electron | Fácil de fazer interface | ~150 MB e muita RAM, ruim para app sempre ligado |

O usuário escolheu **Tauri + Rust**.

## 5. Como funciona (decidido)

### 5.1 Detecção de jogos e saves
- **Banco principal:** manifesto do Ludusavi (`ludusavi-manifest`, gerado da PCGamingWiki). Baixado na primeira execução e atualizado semanalmente, com cópia embutida como reserva para funcionar offline. Traz caminhos de save com marcadores (`<home>`, `<winAppData>`, `<storeUserId>`, `<base>` etc.) e quais nuvens o jogo tem (`cloud.steam`, `cloud.epic`, `cloud.gog`…). Dar crédito (licença) na tela "Sobre".
- **Lojas** (para saber o que está instalado e onde):
  - Steam: `libraryfolders.vdf` + `appmanifest_*.acf`, ID do usuário em `loginusers.vdf`.
  - Epic: manifestos `.item` em `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests`.
  - GOG: registro (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games`).
  - Xbox/Game Pass: saves em `%LOCALAPPDATA%\Packages\*\SystemAppData\wgs`; detectar e marcar como "Xbox Cloud" (restauração desses é limitada, ver 8).
- **Emuladores** (lista própria de caminhos conhecidos, configurável): RetroArch, PCSX2, DuckStation, Dolphin, PPSSPP, RPCS3, Cemu, mGBA e sucessores do Yuzu/Ryujinx. Caminhos portáteis (emulador numa pasta qualquer) são achados procurando os executáveis conhecidos nos discos.
- **Jogos sem loja / não oficiais:** varredura heurística de pastas típicas (`Documents\My Games`, `Saved Games`, `AppData\*`, `Public\Documents`, pasta do próprio jogo) cruzada com o manifesto pelo nome. O que não casar aparece como **"Possível save — confirmar"**; o usuário confirma ou descarta. Também dá pra **adicionar jogo à mão** (nome + pasta do save + .exe opcional).

### 5.2 Prioridade e "já protegido"
Para cada jogo, o app junta os sinais e mostra **onde já existe cópia**:
- Nuvem da loja (do manifesto + o jogo vir daquela loja).
- Pasta do save dentro de pasta sincronizada: OneDrive (registro `HKCU\Software\Microsoft\OneDrive\Accounts\*\UserFolder`, inclusive Documentos/Área de Trabalho redirecionados), Google Drive para computador, Dropbox (`info.json`).
- Backup do próprio SaveGuard em algum destino, com data.

Regra: **Alta** = nenhuma cópia fora do PC. **Média** = só nuvem de loja não confirmada, ou backup do app com mais de 7 dias. **Baixa** = nuvem confirmada ou backup recente. A lista ordena por prioridade e depois pelo último jogado.

### 5.3 Backup contínuo
- **Ao fechar o jogo (principal):** o app observa processos (crate `sysinfo`, a cada ~5 s, consulta barata) procurando os executáveis dos jogos conhecidos (da pasta de instalação da loja ou do .exe informado). Quando o processo termina, espera ~10 s e faz o backup.
- **Vigia de pasta (reserva):** `notify` nas pastas de save; depois de 2 min sem mudanças, faz backup. Pega jogos sem .exe conhecido e saves no meio da partida.
- Enquanto um jogo está aberto, o vigia só marca a pasta como "mudou" e não copia (evita pegar arquivo pela metade e pesar no jogo). A cópia acontece ao fechar.
- **Não duplica:** se o hash (BLAKE3) dos arquivos não mudou desde a última versão, não cria versão nova.
- Prioridade baixa de disco/CPU na cópia.

### 5.4 Formato do backup e versões
```
<destino>\SaveGuard\
  biblioteca.json                 # índice: jogos, versões, origem
  <jogo-id>\
    2026-11-20_21-40-05.zip       # uma versão = um zip
    2026-11-20_21-40-05.json      # caminhos originais com marcadores (<home>…), hashes, tamanho, PC de origem
```
- Caminhos guardados com marcadores, não absolutos, para restaurar num PC com outro nome de usuário ou letra de disco.
- Mantém as últimas **N = 10** versões por jogo (configurável, por jogo também). Versões podem ser **fixadas** (nunca apagadas).

### 5.5 Destinos
- **Primeira execução (assistente):** varredura → resumo "X protegidos, Y em risco" → "Pra onde mandar?". Ele mostra os destinos detectados: pastas do OneDrive/Google Drive/Dropbox, discos externos plugados e "escolher pasta".
- **Automático:** sempre manda para os destinos escolhidos no assistente (pode ser mais de um).
- **Manual** ("Fazer backup agora", por jogo ou de tudo): **pergunta** o destino toda vez, com os padrões pré-marcados.
- **Pendrive/HD:** ao plugar um disco removível, aviso na bandeja: "Copiar todos os saves para <disco>?". Se o disco já tem backup do SaveGuard, oferece "Atualizar backup" (só manda o que mudou).
- Destino ausente (pendrive desplugado, sem internet): a versão fica guardada localmente em `%LOCALAPPDATA%\SaveGuard\pendentes` e é enviada quando o destino voltar.
- **Integração com a nuvem (decidido pelo usuário):** no MVP o app **copia para a pasta sincronizada** do OneDrive / Google Drive para computador / Dropbox e o app oficial faz o envio. Sem login nem tokens dentro do SaveGuard. O app mostra se o cliente oficial está instalado e se a pasta existe. **Login embutido via API fica para depois do MVP** e deve entrar como mais um tipo de destino, sem mudar o formato do backup.

### 5.6 Restauração
- Tela "Restaurar": aponta para um destino (ou detecta sozinho) → lista os jogos do backup → escolhe todos ou alguns → escolhe a versão (padrão: a mais recente).
- Resolve os marcadores para o PC atual. Se o caminho não puder ser resolvido (ex.: jogo de loja não instalado), avisa e deixa escolher a pasta.
- Se já existe save no destino: mostra datas e pergunta; **sempre** guarda o atual como versão antes de sobrescrever.
- Ao extrair, valida cada caminho do zip para não escapar da pasta de destino (proteção contra *zip slip*).

### 5.7 Interface e temas
- Telas: Início (painel), Jogos, Backups (versões), Nuvem/Destinos, Restaurar, Configurações, assistente da primeira execução.
- Bandeja: ícone com estado (protegido / em risco / copiando), menu rápido (backup agora, abrir, pausar).
- Inicia com o Windows (opcional, ligado por padrão), minimizado na bandeja.
- **Temas** (referências visuais em `design/`):
  1. Gamer escuro verde-neon (v1, padrão)
  2. Pastel fofinho (v2a/v2b)
  3. Windows 11 claro, lista ou grade de capas (v3a/v3b)
  4. Cyberpunk neon (v4)
  5. Console (v5): layout próprio com destaque do jogo e carrossel de capas
- Temas 1–4 são o mesmo layout trocando variáveis CSS (cores, fontes, cantos, densidade). O tema 5 troca o componente do Início. A troca acontece na hora e fica salva.
- Capas dos jogos: da Steam (CDN pública por AppID) quando houver; senão um cartão com a inicial e uma cor gerada.
- Detalhes inventados pelo Stitch que **não** entram: "Luna Gamer/Modo Pro", "Criptografia AES-256", "VSS", "LZ4", "SHA-256 validado" como enfeite.

### 5.8 Pilha técnica
- Tauri 2; motor em Rust (`sysinfo`, `notify`, `walkdir`, `zip`, `blake3`, `serde`/`serde_yaml`, `keyring`, `windows` para registro e eventos de disco).
- Interface: Svelte 5 + TypeScript + Tailwind (o Stitch gera Tailwind).
- Dados locais: `%APPDATA%\SaveGuard\config.json` + cache da varredura em SQLite (`rusqlite`).

## 6. Segurança (checklist adaptado; app local, sem servidor)
- Itens 1–3, 9, 11–12, 19 (chaves no servidor, RLS, cookies, rate limit, captcha, HTTPS de servidor): **não se aplicam**, não há servidor nem banco remoto.
- MVP sem login de nuvem, então sem tokens (item 10 não se aplica). Quando vier o login embutido, tokens guardados no **Gerenciador de Credenciais do Windows** (`keyring`), nunca em arquivo nem no repositório. OAuth com PKCE, sem *client secret* embutido.
- Entrada validada no motor: caminhos vindos da interface e dos zips (zip slip, caminhos fora das pastas permitidas).
- Interface: CSP estrita do Tauri, comandos Rust expostos só por lista permitida, nada de `innerHTML` com nome de jogo ou caminho.
- Dependências: `cargo audit` + `npm audit` + Dependabot no GitHub.
- Privacidade: nenhum dado sai do PC, salvo para os destinos que o usuário escolheu e o download do manifesto. Sem telemetria.

## 7. Fora do MVP (ideias para depois)
- Login embutido nas nuvens (OneDrive/Google Drive/Dropbox via API, OAuth com PKCE).
- Linux/Steam Deck.
- Sincronização entre dois PCs.
- Backup na nuvem criptografado com senha.
- Notas por versão ("antes do chefe final").

## 8. A decidir / deixado para quem constrói
- Xbox/Game Pass: os saves `wgs` têm nomes embaralhados e a nuvem do Xbox já cobre. No MVP: detectar, marcar "Xbox Cloud" e fazer backup bruto da pasta, com aviso de que restaurar pode não funcionar. *(minha decisão)*
- Intervalos (5 s de processos, 2 min do vigia, 10 versões, 7 dias para "Média"): valores iniciais, ajustáveis. *(minha decisão)*
- Svelte em vez de React: menor e mais rápido no WebView; troca possível. *(minha decisão)*
- Nome do app e ícone.
