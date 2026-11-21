# Design do SavesGG

Índice de tudo o que foi desenhado para o app, no repositório e no Google Stitch.

## Projetos no Stitch

| Projeto | Id | O que tem |
|---|---|---|
| **SavesGG - backup de saves de jogos** (principal) | `18278923500053803314` | As telas dos 5 temas, o assistente, o design system e uma nota de texto que aponta para este índice |
| **SavesGG - marca (logo)** | `12980498111637929696` | Os 4 conceitos de logo e a exploração de cores do coração |

O Stitch não deixa mover telas de um projeto para outro, nem importar imagens. Por isso a marca continua em projeto próprio, e o projeto principal tem uma nota apontando para ele.

### Telas do projeto principal

| Tela | Id da tela | Cópia local |
|---|---|---|
| Tema 1 - Gamer (Dashboard Principal) | `32d3cbd90df84336a2076a80becce607` | `stitch/v0-dashboard-original.png`, `stitch/v0-gamer.html` |
| Tema 2 - Pastel (versão A) | `5e1e90377d84408a9714fa16ac08f660` | `stitch/v2-pastel-a.*` |
| Tema 2 - Pastel (versão B) | `6694ebc3e99f4b6399853252237bf9ef` | `stitch/v2-pastel-b.*` |
| Tema 3 - Windows 11 claro (lista) | `77cb5fb7ca594876b67b91dc2b46c3ec` | `stitch/v3-win11-claro.*` |
| Tema 3 - Windows 11 (grade de capas) | `e1a88db0baee4ca58115c5cb6270254b` | `stitch/v3-win11-grade.*` |
| Tema 4 - Cyberpunk Neon HUD | `1aa7fdc0a9954b9995c8344077c640da` | `stitch/v4-cyberpunk.*` |
| Tema 5 - Console | `b16cfdb5888347108f3044f037fe70f3` | `stitch/v5-console.*` |
| Assistente - passo 3 (destinos) | `fd52e3a6011544c1b62091e112b95693` | `stitch/v6-assistente.png` |

### Telas do projeto da marca

| Tela | Id da tela | Cópia local |
|---|---|---|
| 4 conceitos de logo (A escudo, B disquete, C cartão GG, D coração) | `cbae2b76db3a433ab2d07571322575a0` | `stitch/marca/v1-conceitos.png` |
| Exploração de cores do conceito D | `1cf9a693bcf44d14871839da70a4392b` | `stitch/marca/v2-cores-stitch.png` |

## Marca escolhida

- **Nome:** SavesGG, que substituiu o nome provisório SaveGuard.
- **Logo:** o conceito D, um coração pixel de "vida extra" com um check dentro.
- **Cores do logo:** vermelho arcade `#ff3b4f`, brilho `#ffd6db` e check `#0d1117`.
- **Arquivos mestres:**
  - `marca/savesgg-icone.svg`: o ícone do app, de onde saem todos os tamanhos em `src-tauri/icons` (`npx tauri icon design/marca/savesgg-icone.svg`).
  - `marca/savesgg-coracao.svg`: só o coração.
- **No código:** o desenho do coração também existe em `src/components/Logo.svelte` e, para a bandeja, em `src-tauri/src/tray.rs`. As três cópias precisam ser iguais.
- **Comparação de cores:** `stitch/marca/v2-cores-coracao.png` e `.html` mostram o mesmo coração em 6 cores. Foi feita aqui porque o Stitch não devolveu a imagem a tempo.
- **Estados na bandeja:** o coração ganha um ponto colorido.
  - verde `#19e6a0`: protegido;
  - amarelo `#ffc233`: em risco;
  - azul `#4d9fff`: copiando;
  - cinza `#9aa5a0`: pausado.

## Capturas do app de verdade (`capturas/`)

- `tema-gamer.png`, `tema-pastel.png`, `tema-win11.png`, `tema-cyberpunk.png`, `tema-console.png`: os 5 temas.
- `assistente-1.png`, `assistente-2.png`: o assistente da primeira execução.
- `savesgg-inicio.png`, `savesgg-titulo.png`: o app depois de trocar o nome e a logo.

## Regra para telas novas

Telas novas passam pelo Stitch, de preferência no projeto principal. A imagem e o HTML aprovados ficam salvos aqui, em `stitch/`, e a tela entra nas tabelas acima.
