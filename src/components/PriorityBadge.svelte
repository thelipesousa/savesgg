<script lang="ts">
  import type { Priority } from "../lib/types";

  let { priority }: { priority: Priority } = $props();

  const texto: Record<Priority, string> = {
    alta: "Prioridade alta",
    media: "Prioridade média",
    baixa: "Prioridade baixa",
  };
</script>

<span class="badge {priority}">{texto[priority]}</span>

<style>
  .badge {
    display: inline-block;
    border-radius: 6px;
    padding: 2px 10px;
    font-size: 0.8em;
    font-weight: 600;
  }
  .alta {
    background: var(--alerta-fundo);
    color: var(--alerta);
  }
  .media {
    background: var(--aviso-fundo);
    color: var(--aviso);
  }
  .baixa {
    background: var(--ok-fundo);
    color: var(--ok);
  }
</style>
