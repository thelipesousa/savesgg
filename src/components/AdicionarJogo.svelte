<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import type { GameView } from "../lib/types";

  interface Props {
    /** Recebe a lista de jogos já com o novo. */
    onadicionado: (jogos: GameView[]) => void;
    onfechar: () => void;
  }
  let { onadicionado, onfechar }: Props = $props();

  let nome = $state("");
  let erro = $state<string | null>(null);
  let salvando = $state(false);

  async function adicionar() {
    if (salvando || nome.trim() === "") return;
    erro = null;
    salvando = true;
    try {
      const lista = await api.addManualGame(nome);
      if (lista) onadicionado(lista);
    } catch (err) {
      erro = errorMessage(err);
    } finally {
      salvando = false;
    }
  }
</script>

<form
  class="cartao adicionar"
  onsubmit={(e) => {
    e.preventDefault();
    adicionar();
  }}
>
  <h2>Adicionar jogo</h2>
  <p class="muted">Para jogos que o SavesGG não achou sozinho. Depois do nome, escolha a pasta dos saves e, se quiser, o .exe.</p>
  <label>
    Nome do jogo
    <input type="text" maxlength="80" bind:value={nome} required />
  </label>
  {#if erro}<p class="erro" role="alert">{erro}</p>{/if}
  <div class="acoes">
    <button type="button" class="botao-secundario" onclick={onfechar}>Cancelar</button>
    <button type="button" class="botao-primario" disabled={salvando || nome.trim() === ""} onclick={adicionar}>
      Escolher pasta do save e adicionar
    </button>
  </div>
</form>

<style>
  .adicionar {
    display: grid;
    gap: 10px;
    margin-bottom: 16px;
  }
  label {
    display: grid;
    gap: 4px;
  }
  .acoes {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }
  .erro {
    color: var(--alerta);
  }
</style>
