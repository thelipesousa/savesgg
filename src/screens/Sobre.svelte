<script lang="ts">
  import { api } from "../lib/api";

  let versao = $state("");
  $effect(() => {
    api.appVersion().then((v) => (versao = v)).catch(() => {});
  });
</script>

<h1>Sobre o SavesGG</h1>
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
</div>

<style>
  h1 {
    margin-bottom: 16px;
  }
  .sobre {
    display: grid;
    gap: 8px;
    max-width: 640px;
  }
</style>
