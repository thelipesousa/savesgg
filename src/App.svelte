<script lang="ts">
  import Inicio from "./screens/Inicio.svelte";
  import Restaurar from "./screens/Restaurar.svelte";
  import Sobre from "./screens/Sobre.svelte";
  import Configuracoes from "./screens/Configuracoes.svelte";

  type Tela = "inicio" | "restaurar" | "configuracoes" | "sobre";
  let tela = $state<Tela>("inicio");

  const menu: { id: Tela; texto: string; icone: string }[] = [
    { id: "inicio", texto: "Início", icone: "🏠" },
    { id: "restaurar", texto: "Restaurar", icone: "↩️" },
    { id: "configuracoes", texto: "Configurações", icone: "⚙️" },
    { id: "sobre", texto: "Sobre", icone: "ℹ️" },
  ];
</script>

<div class="app" data-theme="gamer">
  <nav class="lateral" aria-label="Menu principal">
    <h1 class="marca">🛡️ SaveGuard</h1>
    <ul>
      {#each menu as m (m.id)}
        <li>
          <button type="button" class:ativo={tela === m.id} aria-current={tela === m.id ? "page" : undefined}
            onclick={() => (tela = m.id)}>
            <span aria-hidden="true">{m.icone}</span> {m.texto}
          </button>
        </li>
      {/each}
    </ul>
  </nav>
  <main class="conteudo">
    {#if tela === "inicio"}
      <Inicio />
    {:else if tela === "restaurar"}
      <Restaurar />
    {:else if tela === "configuracoes"}
      <Configuracoes />
    {:else}
      <Sobre />
    {/if}
  </main>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 220px 1fr;
    min-height: 100vh;
  }
  .lateral {
    background: var(--superficie);
    border-right: 1px solid var(--borda);
    padding: 20px 14px;
  }
  .marca {
    font-size: 1.2em;
    margin: 0 8px 24px;
  }
  .lateral ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 4px;
  }
  .lateral button {
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    color: var(--texto-suave);
    padding: 10px 12px;
    border-radius: var(--raio-pequeno);
  }
  .lateral button:hover {
    background: var(--superficie-2);
    color: var(--texto);
  }
  .lateral button.ativo {
    background: rgb(25 230 160 / 0.12);
    color: var(--destaque);
  }
  .conteudo {
    padding: 28px 32px;
    max-width: 1200px;
  }
</style>
