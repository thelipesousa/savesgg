<script lang="ts">
  import Logo from "./components/Logo.svelte";
  import Inicio from "./screens/Inicio.svelte";
  import Restaurar from "./screens/Restaurar.svelte";
  import Sobre from "./screens/Sobre.svelte";
  import Configuracoes from "./screens/Configuracoes.svelte";
  import Assistente from "./screens/Assistente.svelte";

  import AvisoAtualizacao from "./components/AvisoAtualizacao.svelte";
  import { procurarAtualizacao } from "./lib/atualizacao";
  import type { Update } from "@tauri-apps/plugin-updater";
  import { api } from "./lib/api";
  import { aplicarTema } from "./lib/tema";

  type Tela = "inicio" | "restaurar" | "configuracoes" | "sobre";

  let assistente = $state(false);

  aplicarTema("gamer");
  $effect(() => {
    api
      .getSettings()
      .then((s) => {
        aplicarTema(s?.theme ?? "gamer");
        // Só some depois de concluir ou pular (fechar no meio mostra de novo).
        assistente = s?.wizard_done === false;
      })
      .catch(() => {});
  });
  let tela = $state<Tela>("inicio");

  // No app instalado: procura versão nova 30 s depois de abrir e depois a cada 6 h.
  let atualizacao = $state<Update | null>(null);
  $effect(() => {
    if (import.meta.env.DEV) return;
    const procurar = async () => {
      atualizacao = (await procurarAtualizacao()).nova ?? atualizacao;
    };
    const primeiro = setTimeout(procurar, 30_000);
    const repetir = setInterval(procurar, 6 * 60 * 60 * 1000);
    return () => {
      clearTimeout(primeiro);
      clearInterval(repetir);
    };
  });

  const menu: { id: Tela; texto: string; icone: string }[] = [
    { id: "inicio", texto: "Início", icone: "🏠" },
    { id: "restaurar", texto: "Restaurar", icone: "↩️" },
    { id: "configuracoes", texto: "Configurações", icone: "⚙️" },
    { id: "sobre", texto: "Sobre", icone: "ℹ️" },
  ];
</script>

{#if assistente}
  <Assistente onfinish={() => (assistente = false)} />
{:else}
<div class="app">
  <nav class="lateral" aria-label="Menu principal">
    <h1 class="marca"><Logo tamanho={26} /><span>Saves<span class="gg">GG</span></span></h1>
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
    {#if atualizacao}<AvisoAtualizacao {atualizacao} />{/if}
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
{/if}

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
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 1.2em;
    margin: 0 8px 24px;
  }
  .gg {
    color: var(--marca);
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
    background: var(--destaque-fundo);
    color: var(--destaque);
  }
  .conteudo {
    padding: 28px 32px;
    max-width: 1200px;
  }
</style>
