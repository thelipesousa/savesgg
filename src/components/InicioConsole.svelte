<script lang="ts">
  import type { GameView } from "../lib/types";
  import { formatBytes, formatRelative } from "../lib/format";
  import Capa from "./Capa.svelte";
  import PriorityBadge from "./PriorityBadge.svelte";

  interface Props {
    games: GameView[];
    /** Backup do jogo em destaque (abre o seletor de destino). */
    onbackup: (gameId: string) => void;
  }

  let { games, onbackup }: Props = $props();
  let indice = $state(0);
  const atual = $derived(games[Math.min(indice, games.length - 1)]);

  let botoes = $state<HTMLButtonElement[]>([]);

  // Setas trocam o jogo em destaque e levam o foco junto (navegação de console).
  function teclado(e: KeyboardEvent) {
    if (e.key === "ArrowRight") indice = Math.min(indice + 1, games.length - 1);
    else if (e.key === "ArrowLeft") indice = Math.max(indice - 1, 0);
    else return;
    e.preventDefault();
    botoes[indice]?.focus();
  }
</script>

{#if atual}
  <section class="destaque cartao">
    <div class="capa-destaque"><Capa game={atual} tamanho="grande" /></div>
    <div class="info">
      <PriorityBadge priority={atual.priority} />
      <h2>{atual.name}</h2>
      <p class="muted">
        {atual.files} {atual.files === 1 ? "arquivo" : "arquivos"} · {formatBytes(atual.total_bytes)} ·
        último backup {formatRelative(atual.last_backup)}
      </p>
      <button type="button" class="botao-primario" onclick={() => onbackup(atual.id)}>Fazer backup agora</button>
    </div>
  </section>

  <ul class="carrossel" aria-label="Biblioteca">
    {#each games as g, i (g.id)}
      <li class:ativo={i === indice}>
        <button type="button" aria-label={g.name} aria-current={i === indice ? "true" : undefined}
          bind:this={botoes[i]} onclick={() => (indice = i)} onkeydown={teclado}>
          <Capa game={g} tamanho="grande" />
          <span class="nome">{g.name}</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .destaque {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 24px;
    align-items: center;
    background: var(--banner);
    margin-top: 16px;
  }
  .info {
    display: grid;
    gap: 10px;
    justify-items: start;
  }
  .info h2 {
    font-size: 2em;
  }
  .carrossel {
    display: flex;
    gap: 12px;
    overflow-x: auto;
    padding: 12px 2px;
    margin: 12px 0 20px;
    list-style: none;
    scroll-snap-type: x mandatory;
  }
  .carrossel li {
    flex: 0 0 120px;
    scroll-snap-align: start;
  }
  .carrossel button {
    width: 100%;
    display: grid;
    gap: 6px;
    background: none;
    border: 2px solid transparent;
    border-radius: var(--raio-pequeno);
    padding: 4px;
    color: var(--texto);
  }
  .carrossel li.ativo button {
    border-color: var(--destaque);
    box-shadow: var(--brilho);
  }
  .nome {
    font-size: 0.8em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
