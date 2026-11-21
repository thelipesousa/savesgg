<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import type { Destination } from "../lib/types";

  interface Props {
    disco: Destination;
    onclose: () => void;
  }

  let { disco, onclose }: Props = $props();
  let copiando = $state(false);
  let resultado = $state<string | null>(null);

  async function copiar() {
    copiando = true;
    try {
      const r = await api.copyAllTo(disco.id);
      const erros = r.filter((x) => x.status === "erro").length;
      const novos = r.filter((x) => x.status === "criado").length;
      resultado =
        erros > 0
          ? `${novos} copiados, ${erros} com problema (o jogo está aberto?).`
          : `Pronto: ${novos} ${novos === 1 ? "versão copiada" : "versões copiadas"}.`;
    } catch (e) {
      resultado = errorMessage(e);
    } finally {
      copiando = false;
    }
  }
</script>

<div class="cartao aviso-disco" role="status">
  <span class="icone" aria-hidden="true">💾</span>
  <div class="texto">
    <strong>{disco.label} conectado.</strong>
    {#if resultado}
      <span>{resultado}</span>
    {:else if disco.has_backup}
      <span class="muted">Ele já tem um backup do SavesGG. Atualizar com os saves de agora?</span>
    {:else}
      <span class="muted">Copiar todos os saves para ele?</span>
    {/if}
  </div>
  {#if !resultado}
    <button type="button" class="botao-primario" onclick={copiar} disabled={copiando}>
      {copiando ? "Copiando…" : disco.has_backup ? "Atualizar backup" : "Copiar"}
    </button>
    <button type="button" class="botao-secundario" onclick={onclose} disabled={copiando}>Agora não</button>
  {:else}
    <button type="button" class="botao-secundario" onclick={onclose}>Fechar</button>
  {/if}
</div>

<style>
  .aviso-disco {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 16px;
    border-color: rgb(77 159 255 / 0.5);
  }
  .icone {
    font-size: 1.6em;
  }
  .texto {
    display: grid;
    gap: 2px;
    flex: 1;
  }
</style>
