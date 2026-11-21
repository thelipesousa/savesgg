<script lang="ts">
  import type { GameView } from "../lib/types";
  import { formatRelative } from "../lib/format";
  import Capa from "./Capa.svelte";
  import PriorityBadge from "./PriorityBadge.svelte";

  interface Props {
    game: GameView;
    selected: boolean;
    ontoggle: () => void;
    onconfirm?: (keep: boolean) => void;
    /** Só para jogos adicionados à mão. */
    onremove?: () => void;
  }

  let { game, selected, ontoggle, onconfirm, onremove }: Props = $props();
  function remover() {
    if (onremove && confirm(`Remover ${game.name} da lista? Os backups continuam guardados.`)) onremove();
  }

  const pendente = $derived(game.source === "heuristica" && !game.confirmado);
</script>

<li class="cartao-jogo" class:selecionado={selected}>
  <label class="selecao">
    <input type="checkbox" checked={selected} onchange={ontoggle} aria-label={`Selecionar ${game.name}`} />
  </label>
  <Capa {game} tamanho="grande" />
  <h3 title={game.name}>{game.name}</h3>
  <PriorityBadge priority={game.priority} />
  {#if pendente}
    <span class="chip aviso">Possível save — confirmar</span>
    {#if onconfirm}
      <span class="confirmar">
        <button type="button" class="botao-secundario" onclick={() => onconfirm(true)}>Confirmar</button>
        <button type="button" class="botao-perigo" onclick={() => onconfirm(false)}>Descartar</button>
      </span>
    {/if}
  {/if}
  {#if game.last_played}<span class="muted pequeno">Jogado {formatRelative(game.last_played)}</span>{/if}
  <span class="muted pequeno">Último backup: {formatRelative(game.last_backup)}</span>
  {#if game.source === "manual" && onremove}
    <button type="button" class="botao-secundario" onclick={remover}>Remover</button>
  {/if}
</li>

<style>
  .cartao-jogo {
    list-style: none;
    position: relative;
    display: grid;
    gap: 6px;
    padding: 10px;
    background: var(--superficie);
    border: 1px solid var(--borda);
    border-radius: var(--raio);
  }
  .selecionado {
    border-color: var(--destaque);
    box-shadow: var(--brilho);
  }
  .selecao {
    position: absolute;
    top: 16px;
    left: 16px;
    z-index: 1;
  }
  h3 {
    font-size: 0.95em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pequeno {
    font-size: 0.8em;
  }
  .chip.aviso {
    color: var(--aviso);
    border-color: var(--aviso);
    justify-self: start;
  }
  .confirmar {
    display: flex;
    gap: 6px;
  }
  .confirmar button {
    padding: 4px 10px;
    font-size: 0.85em;
  }
</style>
