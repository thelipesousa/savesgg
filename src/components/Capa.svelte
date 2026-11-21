<script lang="ts">
  import type { GameView } from "../lib/types";

  interface Props {
    game: GameView;
    /** "pequena" (lista), "grande" (grade/carrossel) */
    tamanho?: "pequena" | "grande";
  }

  let { game, tamanho = "pequena" }: Props = $props();

  // Sem internet ou sem capa na Steam: cartão com a inicial (nunca ícone quebrado).
  let falhou = $state(false);
  const url = $derived(falhou ? null : game.cover_url);
  const inicial = $derived(game.name.trim().charAt(0).toUpperCase() || "?");
  // Cor estável a partir do id.
  const matiz = $derived([...game.id].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7));
</script>

<div class="capa {tamanho}" style:background={`hsl(${matiz} 45% 30%)`} aria-hidden="true">
  {#if url}
    <img src={url} alt="" loading="lazy" onerror={() => (falhou = true)} />
  {:else}
    <span>{inicial}</span>
  {/if}
</div>

<style>
  .capa {
    display: grid;
    place-items: center;
    overflow: hidden;
    color: #fff;
    font-weight: 700;
    flex-shrink: 0;
  }
  .pequena {
    width: 56px;
    height: 56px;
    border-radius: var(--raio-pequeno);
    font-size: 1.4em;
  }
  .grande {
    width: 100%;
    aspect-ratio: 2 / 3;
    border-radius: var(--raio-pequeno);
    font-size: 3em;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
