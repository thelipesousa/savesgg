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
    background: rgb(255 90 95 / 0.15);
    color: var(--alerta);
  }
  .media {
    background: rgb(255 181 71 / 0.15);
    color: var(--aviso);
  }
  .baixa {
    background: rgb(25 230 160 / 0.12);
    color: var(--ok);
  }
</style>
