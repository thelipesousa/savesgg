<script lang="ts">
  import { formatRelative } from "../lib/format";
  import type { AutoStatusView } from "../lib/types";

  let { status }: { status: AutoStatusView | null } = $props();
</script>

{#if status}
  <p class="status" class:pausado={status.paused}>
    {#if status.paused}
      ⏸ Backup automático pausado
    {:else if status.copying}
      ⏳ Copiando saves…
    {:else if (status.running ?? []).length > 0}
      🎮 Jogando agora: {(status.running ?? []).join(", ")}. Copio quando fechar.
    {:else}
      🛡 Proteção contínua ativa · último backup automático {formatRelative(status.last_backup)}
    {/if}
    {#if status.pending > 0}
      <span class="chip">{status.pending} esperando o destino voltar</span>
    {/if}
  </p>
  {#if status.last_error}
    <p class="erro">Último problema: {status.last_error}</p>
  {/if}
{/if}

<style>
  .status {
    margin: 6px 0 0;
    color: var(--destaque);
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .pausado {
    color: var(--texto-suave);
  }
  .erro {
    color: var(--alerta);
    margin: 4px 0 0;
    font-size: 0.9em;
  }
</style>
