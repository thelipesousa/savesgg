<script lang="ts">
  import type { Update } from "@tauri-apps/plugin-updater";
  import { instalarEReiniciar } from "../lib/atualizacao";
  import { api } from "../lib/api";

  let { atualizacao }: { atualizacao: Update } = $props();

  let pct = $state<number | null>(null);
  let erro = $state<string | null>(null);
  // Portable não tem instalador: a versão nova é baixada na página de downloads.
  let portatil = $state(false);

  $effect(() => {
    api
      .modoPortatil()
      .then((p) => (portatil = p === true))
      .catch(() => {});
  });

  async function baixar() {
    erro = null;
    try {
      await api.abrirPaginaDownload();
    } catch (e) {
      erro = `Não deu para abrir a página: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  async function atualizar() {
    erro = null;
    pct = 0;
    try {
      await instalarEReiniciar(atualizacao, (p) => (pct = p));
    } catch (e) {
      pct = null;
      erro = `Não deu para atualizar: ${e instanceof Error ? e.message : String(e)}`;
    }
  }
</script>

<section class="cartao atualizacao" role="status">
  <div>
    <strong>Versão {atualizacao.version} disponível</strong>
    {#if atualizacao.body}<p class="muted notas">{atualizacao.body}</p>{/if}
    {#if erro}<p class="erro">{erro}</p>{/if}
  </div>
  {#if portatil}
    <button type="button" class="botao-primario" onclick={baixar}>Baixar a versão nova</button>
  {:else}
    <button type="button" class="botao-primario" onclick={atualizar} disabled={pct !== null}>
      {pct === null ? "Atualizar e reiniciar" : `Atualizando… ${pct}%`}
    </button>
  {/if}
</section>

<style>
  .atualizacao {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
    border-color: var(--destaque);
  }
  .notas {
    white-space: pre-line;
    margin: 4px 0 0;
  }
  .erro {
    color: var(--alerta);
  }
</style>
