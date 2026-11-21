<script lang="ts">
  import type { GameView, Store } from "../lib/types";
  import { formatBytes, formatRelative } from "../lib/format";
  import PriorityBadge from "./PriorityBadge.svelte";
  import Capa from "./Capa.svelte";

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


  const lojas: Record<Store, string> = {
    steam: "Steam",
    epic: "Epic",
    gog: "GOG",
    xbox: "Xbox",
    emulador: "Emulador",
    sem_loja: "Sem loja",
  };

  const pendente = $derived(game.source === "heuristica" && !game.confirmado);
</script>

<li class="linha cartao" class:selecionada={selected}>
  <label class="selecao">
    <input type="checkbox" checked={selected} onchange={ontoggle} aria-label={`Selecionar ${game.name}`} />
  </label>
  <Capa {game} />
  <div class="info">
    <div class="titulo">
      <h3>{game.name}</h3>
      <span class="chip">{lojas[game.store]}</span>
      {#if pendente}<span class="chip aviso">Possível save — confirmar</span>{/if}
      {#if game.store === "xbox"}<span class="chip">backup bruto</span>{/if}
    </div>
    <div class="selos">
      <PriorityBadge priority={game.priority} />
      {#if game.protections.length === 0}
        <span class="chip sem">Sem cópia fora do PC</span>
      {/if}
      {#each game.protections as p (p)}
        <span class="chip" class:ok={p !== "Nuvem não confirmada"} class:aviso={p === "Nuvem não confirmada"}>{p}</span>
      {/each}
    </div>
    <div class="caminho mono muted" title={game.save_roots.join("\n")}>{game.save_roots[0] ?? ""}</div>
  </div>
  <div class="numeros">
    <span>{game.files} {game.files === 1 ? "arquivo" : "arquivos"} · {formatBytes(game.total_bytes)}</span>
    {#if game.last_played}<span class="muted">Jogado {formatRelative(game.last_played)}</span>{/if}
    <span class="muted">Último backup: {formatRelative(game.last_backup)}</span>
    {#if game.source === "manual" && onremove}
      <button type="button" class="botao-secundario remover" onclick={remover}>Remover</button>
    {/if}
    {#if pendente && onconfirm}
      <span class="confirmar">
        <button type="button" class="botao-secundario" onclick={() => onconfirm(true)}>Confirmar</button>
        <button type="button" class="botao-perigo" onclick={() => onconfirm(false)}>Descartar</button>
      </span>
    {/if}
  </div>
</li>

<style>
  .linha {
    display: grid;
    grid-template-columns: auto 56px 1fr auto;
    gap: 16px;
    align-items: center;
    padding: 14px 18px;
    list-style: none;
  }
  .selecionada {
    border-color: var(--destaque);
  }
  .titulo {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .titulo h3 {
    font-size: 1.05em;
  }
  .selos {
    display: flex;
    gap: 6px;
    margin: 6px 0;
    flex-wrap: wrap;
  }
  .caminho {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 52ch;
  }
  .numeros {
    display: grid;
    justify-items: end;
    gap: 4px;
    font-size: 0.9em;
  }
  .confirmar {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }
  .remover {
    padding: 4px 12px;
  }
  .confirmar button {
    padding: 4px 12px;
  }
  .chip.aviso {
    color: var(--aviso);
    border-color: var(--aviso);
  }
  .chip.sem {
    color: var(--alerta);
    border-color: var(--alerta);
  }
</style>
