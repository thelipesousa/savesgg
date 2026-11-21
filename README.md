# SavesGG

App para Windows que encontra os saves de todos os seus jogos (Steam, Epic, GOG, Xbox, emuladores e jogos sem loja), mostra quais correm risco por não terem cópia fora do PC e faz backup contínuo para nuvem (pasta do OneDrive/Google Drive/Dropbox), pendrive ou HD externo, com versões e restauração num PC novo.

Feito com **Tauri 2 + Rust** (motor) e **Svelte + TypeScript + Tailwind** (interface).

- Design aprovado: [`docs/superpowers/specs/2026-11-20-saveguard-design.md`](docs/superpowers/specs/2026-11-20-saveguard-design.md)
- Referências visuais (Google Stitch): [`design/stitch/`](design/stitch/)

## Como rodar

Requisitos: Windows 10/11, [Rust](https://rustup.rs) estável, Node 22 e o WebView2 (já vem no Windows).

```powershell
npm install
./scripts/atualizar-manifesto.ps1   # baixa o manifesto de saves embutido no app
npm run tauri dev                   # abre o app em modo de desenvolvimento
```

Testes: `cargo test --workspace` (motor e comandos) e `npm run test -- --run` (interface).
Testes com os dados reais do seu PC (não alteram os saves): veja os comandos `--ignored` nos arquivos
`crates/core/src/scan.rs` e `crates/core/tests/ponta_a_ponta_real.rs`.

## Backup automático

- **Quando copia:** 10 s depois de você fechar o jogo (mesmo se ele travar) ou, para jogos sem executável conhecido, 2 min depois de a pasta do save parar de mudar. Com o jogo aberto, nada é copiado: a cópia espera o fechamento para não pesar e não pegar arquivo pela metade.
- **Para onde:** os destinos marcados em **Configurações → Destinos do backup automático**. O backup manual ("Fazer backup dos selecionados") continua perguntando o destino toda vez.
- **Destino fora do ar** (pendrive desplugado, pasta da nuvem sumida): o backup fica na fila em `%LOCALAPPDATA%\SavesGG\pendentes` e é enviado sozinho quando o destino volta.
- **Bandeja:** fechar a janela deixa o SavesGG na bandeja. O ícone mostra o estado (o coração da logo com um ponto: verde protegido, amarelo com jogo sem cópia fora do PC, azul copiando, cinza pausado) e o menu tem backup agora, pausar/retomar, copiar tudo para um disco plugado e sair.
- **Pendrive/HD:** ao plugar, aparece uma notificação e um aviso no Início para copiar tudo (ou atualizar o backup que já está nele).
- **Iniciar com o Windows:** ligado por padrão, abrindo minimizado na bandeja. Desligue em **Configurações → Iniciar com o Windows**. Só vale no app instalado (em desenvolvimento não é registrado).
- **Possíveis saves** (heurística) e jogos acima de 2 GB só entram no automático depois de confirmados.

## Temas e assistente

- **Assistente da primeira execução:** ao abrir pela primeira vez, o SavesGG escaneia o PC, mostra quantos jogos já estão protegidos e quantos não têm cópia fora do PC, pergunta para onde mandar os saves (destinos do backup automático) e deixa escolher o tema. Dá para pular; ele não volta depois de concluído.
- **5 temas**, trocados na hora em **Configurações → Aparência**: Gamer (escuro verde-neon), Pastel (claro e fofinho), Windows 11 (claro e limpo), Cyberpunk (neon ciano e rosa) e Console (jogo em destaque e carrossel de capas, navegável pelas setas).
- **Lista ou grade de capas** para os jogos, também em Aparência.
- **Capas:** vêm da Steam pelo AppID do jogo (`cdn.cloudflare.steamstatic.com`). Sem internet ou sem capa, aparece um cartão com a inicial do jogo. As referências visuais (Google Stitch) e capturas reais ficam em `design/`.

## Restaurar, versões e jogos à mão

- **Restaurar vários jogos de uma vez:** na tela Restaurar, escolha o destino, deixe marcados os jogos e clique em "Restaurar selecionados". Cada jogo volta na versão mais recente que não seja de segurança; antes de substituir um save, o app guarda o atual numa **versão de segurança** (fixada, com selo próprio). Jogos com arquivo sem lugar neste PC aparecem em "precisa de você" para escolher a pasta.
- **Backup em outra pasta:** "Procurar backup em outra pasta…" aceita a pasta que contém `SavesGG`, a própria `SavesGG` ou a raiz da nuvem com `SavesGG Backups`. Vale só até fechar o app (não vira destino automático).
- **Versões:** fixe ou desafixe cada versão (fixadas nunca são apagadas). Quantas guardar: no geral em Configurações e por jogo na tela Restaurar (de 1 a 100).
- **Jogo à mão:** "Adicionar jogo" no Início (nome, pasta do save e, se quiser, o .exe). "Remover" tira da lista e mantém os backups. Possíveis saves descartados podem voltar em Configurações.
- **Aviso da nuvem:** se o OneDrive/Google Drive/Dropbox estiver fechado, o app avisa que os backups não estão subindo.
- **Log:** `%LOCALAPPDATA%\com.savesgg.app\logs\savesgg.log` (botão "Abrir pasta de logs" em Sobre).

## Atualização automática e lançamentos

O app instalado procura versão nova 30 s depois de abrir e a cada 6 h (também em Sobre → "Procurar atualização"). As versões ficam no repositório público [`thelipesousa/savesgg-releases`](https://github.com/thelipesousa/savesgg-releases); o código continua privado. Toda atualização é **assinada**: o app recusa arquivo sem assinatura válida.

**Chave de assinatura:** `%USERPROFILE%\.tauri\savesgg.key` (fora do repositório; `*.key` está no `.gitignore`). **Guarde uma cópia em lugar seguro** (ex.: gerenciador de senhas): sem ela não há como publicar atualizações para quem já instalou. A chave pública está em `src-tauri/tauri.conf.json`.

**Lançar uma versão** (na `main`, depois do PR `dev` → `main`):

```powershell
powershell -File scripts/lancar.ps1 -Notas "O que mudou nesta versão"
```

O script compila, assina, gera `latest.json` e `SHA256SUMS.txt` e publica a release `vX.Y.Z` em `savesgg-releases`, com o instalador em nome estável (`SavesGG_x64-setup.exe`). Para testar sem publicar: `-Ensaio`. Antes, suba a versão em `tauri.conf.json`, `Cargo.toml` (workspace) e `package.json`.

## Fluxo de branches

- `main`: produção e branch padrão; só código estável, é dela que sai o release.
- `dev`: desenvolvimento e integração; recebe as funcionalidades prontas.
- `feature/<nome>`: uma funcionalidade por branch, criada a partir da `dev`.
- `fix/<nome>`: correção de bug, a partir da `dev`.
- `hotfix/<nome>`: correção urgente em produção, a partir da `main`; depois volta pra `main` e pra `dev`.

Commits no padrão [Conventional Commits](https://www.conventionalcommits.org/pt-br/) (`feat`, `fix`, `docs`…), em português. PRs vão para a `dev`; lançamento é PR `dev` → `main`.

## Créditos

Locais de saves baseados no [ludusavi-manifest](https://github.com/mtkennerly/ludusavi-manifest) (dados da PCGamingWiki).
