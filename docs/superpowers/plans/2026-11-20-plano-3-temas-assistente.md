# SaveGuard — Plano 3: temas, assistente da primeira execução e capas

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** O app ganha os 5 temas alternáveis na hora (gamer, pastel, Windows 11, cyberpunk, console), lista ou grade de capas, capas da Steam quando houver e um assistente na primeira execução que escaneia, mostra o risco, escolhe os destinos automáticos e o tema.

**Architecture:** Temas são conjuntos de variáveis CSS sob `[data-theme="…"]` aplicados no `<html>`; só o tema console troca o componente do Início (destaque + carrossel). O motor ganha o AppID da Steam por jogo (do manifesto ou da loja) e a interface monta a URL da capa no CDN da Steam, caindo no cartão com a inicial quando não há capa ou sem internet. A configuração guarda `theme`, `game_view` e `wizard_done`; comandos novos validam os valores.

**Tech Stack:** o dos planos 1 e 2 (sem dependências novas).

**Spec:** `docs/superpowers/specs/2026-11-20-saveguard-design.md` (seções 5.5 e 5.7). Referências visuais: `design/stitch/` (v0–v5 temas, v6 assistente). Issue: #9 (parte de #1).

## Global Constraints

- Temas (ids fixos): `gamer` (padrão), `pastel`, `win11`, `cyberpunk`, `console`. Qualquer outro valor vindo da interface é recusado.
- Exibição dos jogos: `lista` ou `grade`; padrão `lista` (o tema win11 sugere grade, mas a escolha é do usuário).
- Paletas (do HTML do Stitch): gamer `#0b100e/#131a17/#19e6a0`; pastel `#f4effc/#ffffff/#9b87f5`; win11 `#f3f3f3/#ffffff/#0067c0`; cyberpunk `#07090e/#0c1220/#00f0ff` com alerta `#ff007f`; console `#10131a/#1d2026/#dbb8ff`.
- Sem fontes externas (CSP `default-src 'self'` e app offline): fontes do sistema por tema (Segoe UI Variable, Cascadia).
- Capas: `https://cdn.cloudflare.steamstatic.com/steam/apps/<appid>/library_600x900.jpg` (já liberado no `img-src` da CSP). Falhou ou não há AppID → cartão com a inicial.
- Assistente aparece quando `wizard_done == false`; pode ser pulado; ao terminar grava `wizard_done = true`. Não registra nada no Windows.
- Mesmo padrão de commits, idioma e segurança dos planos anteriores. Branch `feature/9-temas-assistente`; PR para `dev` (fechar #9 à mão depois do merge, já que a base não é a `main`).

## Review Focus

1. **Sem internet:** capas não carregam → cartão com a inicial, sem ícone de imagem quebrada. Teste na Task 3.
2. **Config antiga ou editada à mão com tema inválido** (`"theme": "azul"`): o app usa `gamer`, sem tela em branco. Teste na Task 1.
3. **Usuário fecha a janela no meio do assistente:** na próxima abertura ele volta (só some depois de concluir ou pular). Teste na Task 5.
4. **Assistente sem nenhum destino disponível** (sem OneDrive nem pendrive): dá para adicionar pasta ou seguir sem destino, com aviso claro. Teste na Task 5.
5. **Tema claro (pastel/win11):** textos e selos continuam legíveis (contraste das cores de prioridade). Coberto na Task 2 (tokens de prioridade por tema, não cores fixas).

---

### Task 1: Configuração de aparência e AppID da Steam

**Files:** Modify `crates/core/src/config.rs`, `crates/core/src/scan.rs`, `src-tauri/src/views.rs`, `src-tauri/src/commands.rs`, `src-tauri/build.rs`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`.

**Interfaces:**
- Produces:
  - `pub const TEMAS: [&str; 5] = ["gamer", "pastel", "win11", "cyberpunk", "console"];` e `pub const EXIBICOES: [&str; 2] = ["lista", "grade"];` em `config.rs`.
  - `Config` ganha `game_view: String` (padrão `"lista"`) e `wizard_done: bool` (padrão `false`); `pub fn theme_or_default(&self) -> &str` e `pub fn game_view_or_default(&self) -> &str` devolvem o padrão quando o valor salvo não é válido.
  - `DetectedGame.steam_app_id: Option<u32>`: do `steam_id` do manifesto, ou do `store_game_id` quando a loja é Steam.
  - `GameView.cover_url: Option<String>` (URL do CDN quando há AppID).
  - Comandos: `set_theme(theme)`, `set_game_view(view)` (recusam valor fora da lista), `complete_wizard()`; `get_settings` ganha `theme`, `game_view`, `wizard_done`.
- [ ] **Step 1: Testes**
  - `tema_invalido_usa_padrao`: config com `"theme": "azul"` → `theme_or_default() == "gamer"`; `"game_view": "mosaico"` → `"lista"`
  - `config_antiga_mostra_assistente`: JSON sem `wizard_done` → `false`
  - `appid_vem_do_manifesto`: varredura do mini-manifesto com Hades II sem loja → `steam_app_id == Some(1145350)`
  - `game_view_tem_capa`: `steam_app_id = Some(1245620)` → `cover_url` termina em `/1245620/library_600x900.jpg`; sem AppID → `None`
  - `valida_tema_e_exibicao` (em `commands.rs`, função pura `validate_choice`): `"pastel"` aceito, `"azul"` recusado
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(interface): guarda tema, exibição e assistente na configuração`.

### Task 2: Os 5 temas

**Files:** Create `src/theme/temas.css`, `src/lib/tema.ts`, `src/lib/tema.test.ts`; Modify `src/theme/gamer.css`, `src/app.css`, `src/App.svelte`, `src/screens/Configuracoes.svelte`, `src/components/PriorityBadge.svelte`, `src/components/GameRow.svelte`, `src/lib/api.ts`, `src/lib/types.ts`.

**Interfaces:**
- `temas.css`: um bloco `[data-theme="…"]` por tema redefinindo `--fundo --superficie --superficie-2 --borda --texto --texto-suave --destaque --destaque-texto --alerta --aviso --ok --raio --raio-pequeno --fonte --fonte-titulo --brilho` e os novos `--alerta-fundo --aviso-fundo --ok-fundo` (fundos dos selos de prioridade).
- `tema.ts`: `export const TEMAS: { id, nome, descricao, amostra: [fundo, superficie, destaque] }[]` e `export function aplicarTema(id: string)` (põe `data-theme` no `<html>`; id desconhecido vira `gamer`).
- Configurações ganha a seção **Aparência**: cartões com a amostra de cada tema (o atual marcado) e o seletor **Lista / Grade**.
- O App lê `get_settings` ao abrir e aplica o tema; a troca vale na hora, sem recarregar.
- `PriorityBadge`/`GameRow` usam os tokens (sem `rgb(...)` fixo).
- [ ] **Step 1: Testes**
  - `aplicar_tema_marca_o_html`: `aplicarTema("pastel")` → `document.documentElement.dataset.theme == "pastel"`; `aplicarTema("azul")` → `"gamer"`
  - `escolher_tema_salva_e_aplica` (Configurações): clicar no cartão "Cyberpunk" chama `set_theme` com `"cyberpunk"` e muda o `data-theme`
  - `escolher_grade_salva`: clicar em "Grade" chama `set_game_view` com `"grade"`
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar + `npm run check`. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(interface): adiciona os 5 temas alternáveis na hora`.

### Task 3: Capas e grade de jogos

**Files:** Create `src/components/Capa.svelte`, `src/components/GameCard.svelte`, `src/components/Capa.test.ts`; Modify `src/components/GameRow.svelte`, `src/screens/Inicio.svelte`.

**Interfaces:**
- `Capa.svelte` (`{ game, tamanho }`): `<img alt="" loading="lazy">` com `cover_url`; em `onerror` (ou sem URL) mostra o cartão com a inicial e a cor estável já usada.
- `GameCard.svelte`: cartão vertical para a grade (capa grande, nome, selo de prioridade, último backup, checkbox).
- Início usa `GameRow` (lista) ou `GameCard` em grade conforme `game_view`.
- [ ] **Step 1: Testes**
  - `capa_com_url_mostra_imagem`: `cover_url` presente → `img` com o `src`
  - `capa_que_falha_vira_inicial`: disparar `error` na `img` → some a imagem e aparece a inicial
  - `sem_appid_mostra_inicial`
  - `grade_renderiza_cartoes`: `GameCard` mostra nome, selo e checkbox com rótulo "Selecionar <jogo>"
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(interface): mostra capas dos jogos e exibição em grade`.

### Task 4: Início do tema console

**Files:** Create `src/components/InicioConsole.svelte`, `src/components/InicioConsole.test.ts`; Modify `src/screens/Inicio.svelte`.

**Interfaces:**
- Com `data-theme="console"`, o topo do Início vira: destaque do jogo selecionado (capa grande, nome, prioridade, "Fazer backup agora") e o carrossel horizontal de capas (`role="list"`, nome acessível "Biblioteca"), navegável por teclado (setas). A lista detalhada continua abaixo.
- [ ] **Step 1: Testes**
  - `carrossel_lista_os_jogos`: 3 jogos → 3 itens em `getByRole("list", { name: "Biblioteca" })`
  - `selecionar_no_carrossel_troca_destaque`: clicar no 2º item → o destaque mostra o nome dele
  - `seta_direita_avanca`: tecla `ArrowRight` no carrossel seleciona o próximo
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(interface): adiciona o início em estilo console`.

### Task 5: Assistente da primeira execução

**Files:** Create `src/screens/Assistente.svelte`, `src/screens/Assistente.test.ts`; Modify `src/App.svelte`.

**Interfaces:**
- Passos (visual de `design/stitch/v6-assistente.png`): **Boas-vindas** → **Seus jogos** (roda `scan`, mostra "Encontrei N jogos: X já protegidos, Y sem cópia fora do PC") → **Para onde mandar** (cartões de destino com checkbox, "Escolher outra pasta…", aviso se nada estiver marcado; grava `set_auto_destinations`) → **Tema** (cartões dos 5 temas, aplica na hora; grava `set_theme`) → `complete_wizard` e vai para o Início.
- Indicador de 4 passos no topo; **Voltar**, **Continuar** e **Pular configuração** (também chama `complete_wizard`).
- O App mostra o assistente no lugar do menu enquanto `wizard_done` for `false`.
- [ ] **Step 1: Testes**
  - `fluxo_completo`: avançar pelos passos chama `scan`, `set_auto_destinations` (com o destino marcado), `set_theme` e `complete_wizard`, nessa ordem
  - `resumo_da_varredura`: 3 jogos (1 baixa, 2 alta) → texto "1 já protegido" e "2 sem cópia fora do PC"
  - `sem_destino_mostra_aviso_e_deixa_seguir`: sem destinos → aviso visível e **Continuar** habilitado
  - `pular_conclui`: **Pular configuração** chama `complete_wizard`
  - `app_mostra_assistente_quando_nao_concluido`: `get_settings` com `wizard_done: false` → App renderiza o assistente; `true` → o menu
- [ ] **Step 2:** Rodar. Esperado: FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Rodar + `npm run check`. Esperado: PASS.
- [ ] **Step 5:** Commit `feat(interface): adiciona o assistente da primeira execução`.

### Task 6: Verificação real e documentação

- [ ] **Step 1:** `npm run tauri dev` com a configuração apagada: o assistente aparece; capturar cada passo; concluir; capturar o Início em cada um dos 5 temas e a grade. Restaurar o estado (apagar a config de teste).
- [ ] **Step 2:** Mandar as capturas ao usuário.
- [ ] **Step 3:** README: seção "Temas e assistente".
- [ ] **Step 4:** Commit `docs: explica temas, grade de capas e assistente`. Push e PR para `dev` com o checklist de segurança.
