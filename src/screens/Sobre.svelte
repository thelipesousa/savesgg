<script lang="ts">
  import Logo from "../components/Logo.svelte";
  import { api, errorMessage } from "../lib/api";

  let versao = $state("");
  let erro = $state<string | null>(null);

  async function abrirLogs() {
    erro = null;
    try {
      await api.openLogsFolder();
    } catch (e) {
      erro = errorMessage(e);
    }
  }
  $effect(() => {
    api.appVersion().then((v) => (versao = v)).catch(() => {});
  });
</script>

<h1 class="titulo-sobre"><Logo tamanho={36} /> Sobre o SavesGG</h1>
<div class="cartao sobre">
  <p>Versão {versao}</p>
  <p>
    Encontra os saves dos seus jogos e faz backup para a nuvem, um pendrive ou um HD externo. Nenhum dado
    sai do seu PC, a não ser para os destinos que você escolher.
  </p>
  <p class="muted">
    Locais de saves baseados no <strong>ludusavi-manifest</strong> (licença MIT), com dados da
    <strong>PCGamingWiki</strong>. Obrigado a esses projetos.
  </p>
  <div class="acoes">
    <button type="button" class="botao-secundario" onclick={abrirLogs}>Abrir pasta de logs</button>
  </div>
  {#if erro}<p class="erro" role="alert">{erro}</p>{/if}
</div>

<style>
  .titulo-sobre {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h1 {
    margin-bottom: 16px;
  }
  .acoes {
    display: flex;
    gap: 10px;
  }
  .sobre {
    display: grid;
    gap: 8px;
    max-width: 640px;
  }
</style>
