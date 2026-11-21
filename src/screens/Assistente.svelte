<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import { formatBytes } from "../lib/format";
  import { TEMAS, aplicarTema } from "../lib/tema";
  import type { Destination, GameView } from "../lib/types";

  let { onfinish }: { onfinish: () => void } = $props();

  const PASSOS = ["Boas-vindas", "Seus jogos", "Para onde mandar", "Tema"];
  let passo = $state(0);
  let ocupado = $state(false);
  let erro = $state<string | null>(null);

  let jogos = $state<GameView[] | null>(null);
  let destinos = $state<Destination[]>([]);
  let marcados = $state<string[]>([]);
  let tema = $state("gamer");

  const visiveis = $derived((jogos ?? []).filter((g) => !g.grande));
  const protegidos = $derived(visiveis.filter((g) => g.priority === "baixa").length);
  const emRisco = $derived(visiveis.filter((g) => g.priority === "alta").length);

  async function tentar(acao: () => Promise<void>) {
    ocupado = true;
    erro = null;
    try {
      await acao();
    } catch (e) {
      erro = errorMessage(e);
    } finally {
      ocupado = false;
    }
  }

  function comecar() {
    passo = 1;
    tentar(async () => {
      jogos = await api.scan();
    });
  }

  function irParaDestinos() {
    passo = 2;
    tentar(async () => {
      destinos = await api.listDestinations();
    });
  }

  function alternar(id: string) {
    marcados = marcados.includes(id) ? marcados.filter((x) => x !== id) : [...marcados, id];
  }

  function outraPasta() {
    tentar(async () => {
      const nova = await api.addCustomFolder();
      if (nova) {
        destinos = await api.listDestinations();
        if (!marcados.includes(nova.id)) marcados = [...marcados, nova.id];
      }
    });
  }

  function irParaTema() {
    tentar(async () => {
      await api.setAutoDestinations(marcados);
      passo = 3;
    });
  }

  function escolherTema(id: string) {
    tema = aplicarTema(id);
  }

  function concluir() {
    tentar(async () => {
      await api.setTheme(tema);
      await api.completeWizard();
      onfinish();
    });
  }

  function pular() {
    tentar(async () => {
      await api.completeWizard();
      onfinish();
    });
  }

  function icone(d: Destination): string {
    if (d.kind.tipo === "pasta_sincronizada") return "☁️";
    if (d.kind.tipo === "disco_removivel") return "💾";
    return "📁";
  }
</script>

<div class="fundo">
  <div class="janela">
    <header class="topo">
      <span class="marca">🛡️ Save<span class="verde">Guard</span></span>
      <button type="button" class="link" onclick={pular} disabled={ocupado}>Pular configuração</button>
    </header>

    <ol class="progresso" aria-label="Passos">
      {#each PASSOS as nome, i (nome)}
        <li class:feito={i < passo} class:atual={i === passo} aria-current={i === passo ? "step" : undefined}>
          <span class="bolinha">{i < passo ? "✓" : i + 1}</span>
          <span>{nome}</span>
        </li>
      {/each}
    </ol>

    <main class="conteudo">
      {#if erro}<p class="erro" role="alert">{erro}</p>{/if}

      {#if passo === 0}
        <h1>Bem-vindo ao SaveGuard</h1>
        <p class="muted">
          Vou procurar os saves dos seus jogos neste PC, mostrar quais não têm cópia fora dele e copiar
          sozinho sempre que você fechar um jogo. Leva só um minuto.
        </p>
      {:else if passo === 1}
        <h1>Seus jogos</h1>
        {#if jogos === null}
          <p class="muted" role="status">Procurando saves no PC… (pode levar alguns segundos)</p>
        {:else}
          <div class="resumo">
            <span>Encontrei <strong>{visiveis.length} {visiveis.length === 1 ? "jogo" : "jogos"}</strong>:</span>
            <span class="chip ok">{protegidos} {protegidos === 1 ? "já protegido" : "já protegidos"}</span>
            <span class="chip risco">{emRisco} sem cópia fora do PC</span>
          </div>
          <ul class="jogos">
            {#each visiveis.slice(0, 8) as g (g.id)}
              <li>{g.name}</li>
            {/each}
            {#if visiveis.length > 8}<li class="muted">e mais {visiveis.length - 8}…</li>{/if}
          </ul>
        {/if}
      {:else if passo === 2}
        <h1>Para onde mandar os saves?</h1>
        <p class="muted">Escolha um ou mais destinos. Uma pasta na nuvem ou um pendrive protegem contra formatação.</p>
        <ul class="destinos">
          {#each destinos as d (d.id)}
            <li>
              <label class="destino" class:marcado={marcados.includes(d.id)} class:indisponivel={!d.available}>
                <span class="icone" aria-hidden="true">{icone(d)}</span>
                <span class="texto">
                  <strong>{d.label}</strong>
                  <span class="muted">
                    {#if d.kind.tipo === "pasta_sincronizada"}Pasta sincronizada · recomendado{:else if d.kind.tipo === "disco_removivel"}Disco externo{:else}Pasta neste PC{/if}
                    {#if d.free_bytes !== null}· {formatBytes(d.free_bytes)} livres{/if}
                  </span>
                </span>
                <input type="checkbox" checked={marcados.includes(d.id)} disabled={!d.available}
                  onchange={() => alternar(d.id)} aria-label={d.label} />
              </label>
            </li>
          {/each}
          <li>
            <button type="button" class="destino tracejado" onclick={outraPasta} disabled={ocupado}>
              <span class="icone" aria-hidden="true">＋</span>
              <span class="texto"><strong>Escolher outra pasta…</strong><span class="muted">HD externo ou outra pasta</span></span>
            </button>
          </li>
        </ul>
        {#if marcados.length === 0}
          <p class="aviso" role="status">
            Nenhum destino marcado: dá para seguir, mas o backup automático só começa depois que você escolher um em
            Configurações.
          </p>
        {/if}
        <p class="dica">O backup automático manda para cá sempre que você fechar um jogo. Dá para mudar depois em Configurações.</p>
      {:else}
        <h1>Escolha um tema</h1>
        <div class="temas" role="radiogroup" aria-label="Tema">
          {#each TEMAS as t (t.id)}
            <label class="tema" class:marcado={tema === t.id}>
              <input type="radio" name="tema-assistente" checked={tema === t.id} onchange={() => escolherTema(t.id)}
                aria-label={t.nome} />
              <span class="amostra" aria-hidden="true" style:background={t.amostra[0]}>
                <span style:background={t.amostra[1]}></span>
                <span class="ponto" style:background={t.amostra[2]}></span>
              </span>
              <strong>{t.nome}</strong>
              <span class="muted">{t.descricao}</span>
            </label>
          {/each}
        </div>
      {/if}
    </main>

    <footer class="rodape">
      {#if passo > 0}
        <button type="button" class="botao-secundario" onclick={() => (passo = passo - 1)} disabled={ocupado}>Voltar</button>
      {/if}
      <span class="espaco"></span>
      {#if passo === 0}
        <button type="button" class="botao-primario" onclick={comecar}>Começar</button>
      {:else if passo === 1}
        <button type="button" class="botao-primario" onclick={irParaDestinos} disabled={ocupado || jogos === null}>Continuar</button>
      {:else if passo === 2}
        <span class="muted pequeno">{marcados.length} {marcados.length === 1 ? "destino" : "destinos"}</span>
        <button type="button" class="botao-primario" onclick={irParaTema} disabled={ocupado}>Continuar</button>
      {:else}
        <button type="button" class="botao-primario" onclick={concluir} disabled={ocupado}>Concluir</button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .fundo {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 24px;
    background: radial-gradient(circle at 50% 0%, var(--ok-fundo), transparent 65%), var(--fundo);
  }
  .janela {
    width: min(860px, 100%);
    background: var(--superficie);
    border: 1px solid var(--borda);
    border-radius: calc(var(--raio) * 1.5);
    box-shadow: var(--brilho);
    overflow: hidden;
    display: grid;
  }
  .topo {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 24px;
    border-bottom: 1px solid var(--borda);
  }
  .marca {
    font-weight: 800;
  }
  .verde {
    color: var(--destaque);
  }
  .link {
    background: none;
    border: 0;
    color: var(--texto-suave);
    text-decoration: underline;
  }
  .progresso {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    list-style: none;
    padding: 20px 24px;
    margin: 0;
    border-bottom: 1px solid var(--borda);
  }
  .progresso li {
    display: grid;
    justify-items: center;
    gap: 6px;
    font-size: 0.8em;
    color: var(--texto-suave);
  }
  .bolinha {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    border: 1px solid var(--borda);
    background: var(--superficie-2);
    font-weight: 700;
  }
  .feito .bolinha {
    background: var(--destaque);
    color: var(--destaque-texto);
    border-color: var(--destaque);
  }
  .atual {
    color: var(--destaque) !important;
    font-weight: 700;
  }
  .atual .bolinha {
    border: 2px solid var(--destaque);
    box-shadow: var(--brilho);
  }
  .conteudo {
    padding: 28px 32px;
    display: grid;
    gap: 14px;
    min-height: 320px;
    align-content: start;
  }
  .conteudo h1 {
    font-size: 1.8em;
  }
  .resumo {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    padding: 12px 16px;
    background: var(--superficie-2);
    border: 1px solid var(--borda);
    border-radius: var(--raio-pequeno);
  }
  .chip.risco {
    color: var(--aviso);
    border-color: var(--aviso);
  }
  .jogos {
    columns: 2;
    margin: 0;
    padding-left: 20px;
  }
  .destinos {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 10px;
  }
  .destino {
    width: 100%;
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 14px;
    align-items: center;
    padding: 14px 16px;
    background: var(--superficie-2);
    border: 1px solid var(--borda);
    border-radius: var(--raio);
    color: var(--texto);
    text-align: left;
    cursor: pointer;
  }
  .destino.marcado {
    border: 2px solid var(--destaque);
    box-shadow: var(--brilho);
  }
  .destino.indisponivel {
    opacity: 0.5;
  }
  .tracejado {
    border-style: dashed;
    background: transparent;
  }
  .icone {
    font-size: 1.5em;
  }
  .texto {
    display: grid;
    gap: 2px;
  }
  .dica {
    font-size: 0.85em;
    color: var(--texto-suave);
  }
  .aviso {
    color: var(--aviso);
  }
  .erro {
    color: var(--alerta);
  }
  .temas {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 12px;
  }
  .tema {
    display: grid;
    gap: 4px;
    padding: 10px;
    border: 1px solid var(--borda);
    border-radius: var(--raio-pequeno);
    background: var(--superficie-2);
    cursor: pointer;
    position: relative;
  }
  .tema.marcado {
    border: 2px solid var(--destaque);
    box-shadow: var(--brilho);
  }
  .tema input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .tema:has(input:focus-visible) {
    outline: 2px solid var(--destaque);
    outline-offset: 2px;
  }
  .amostra {
    display: flex;
    gap: 6px;
    align-items: center;
    height: 48px;
    border-radius: 8px;
    padding: 8px;
  }
  .amostra span {
    flex: 1;
    height: 100%;
    border-radius: 6px;
  }
  .amostra .ponto {
    flex: 0 0 18px;
    height: 18px;
    border-radius: 50%;
  }
  .rodape {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 16px 24px;
    border-top: 1px solid var(--borda);
    background: var(--superficie-2);
  }
  .pequeno {
    font-size: 0.85em;
  }
</style>
