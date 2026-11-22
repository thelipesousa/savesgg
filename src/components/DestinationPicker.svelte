<script lang="ts">
  import AvisoNuvem from "./AvisoNuvem.svelte";
  import type { Destination } from "../lib/types";
  import { formatBytes } from "../lib/format";

  interface Props {
    destinations: Destination[];
    preselected: string[];
    onconfirm: (ids: string[]) => void;
    oncancel: () => void;
    onaddfolder: () => void;
  }

  let { destinations, preselected, onconfirm, oncancel, onaddfolder }: Props = $props();

  let selected = $state<string[]>([]);
  $effect.pre(() => {
    selected = preselected.filter((id) => destinations.some((d) => d.id === id && d.available));
  });

  function toggle(id: string) {
    selected = selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id];
  }

  function icon(d: Destination): string {
    if (d.kind.tipo === "pasta_sincronizada" || d.kind.tipo === "nuvem") return "☁️";
    if (d.kind.tipo === "disco_removivel") return "💾";
    return "📁";
  }
</script>

<div class="modal-fundo" role="presentation" onclick={oncancel}>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="titulo-destino"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.key === "Escape" && oncancel()}
  >
    <h2 id="titulo-destino">Para onde mandar os saves?</h2>
    {#if destinations.length === 0}
      <p class="muted">Nenhum destino encontrado. Adicione uma pasta ou plugue um pendrive.</p>
    {/if}
    <ul class="lista-destinos">
      {#each destinations as d (d.id)}
        <li class:indisponivel={!d.available}>
          <label>
            <input
              type="checkbox"
              checked={selected.includes(d.id)}
              disabled={!d.available}
              onchange={() => toggle(d.id)}
              aria-label={d.label}
            />
            <span class="icone" aria-hidden="true">{icon(d)}</span>
            <span class="nome">{d.label}</span>
            <span class="detalhes">
              {#if !d.available}indisponível{/if}
              {#if d.free_bytes !== null}{formatBytes(d.free_bytes)} livres{/if}
              {#if d.has_backup}<span class="chip">já tem backup</span>{/if}
            </span>
          </label>
          <AvisoNuvem destino={d} />
        </li>
      {/each}
    </ul>
    <div class="acoes">
      <button type="button" class="botao-secundario" onclick={onaddfolder}>Adicionar pasta…</button>
      <span class="espaco"></span>
      <button type="button" class="botao-secundario" onclick={oncancel}>Cancelar</button>
      <button
        type="button"
        class="botao-primario"
        disabled={selected.length === 0}
        onclick={() => onconfirm(selected)}
      >
        Copiar
      </button>
    </div>
  </div>
</div>
