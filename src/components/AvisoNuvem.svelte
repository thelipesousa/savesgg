<script lang="ts">
  import type { Destination } from "../lib/types";

  // Aviso quando o app oficial da nuvem está fechado: os backups chegam à pasta, mas não sobem.
  let { destino }: { destino: Destination } = $props();
</script>

{#if destino.cliente_ativo === false}
  <span class="aviso-nuvem">
    O {destino.label} não está aberto: os backups ficam na pasta, mas só sobem quando ele abrir.
  </span>
{/if}

<style>
  .aviso-nuvem {
    display: block;
    color: var(--aviso);
    font-size: 0.85em;
    margin-top: 2px;
  }
</style>
