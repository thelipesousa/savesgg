<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import { formatBytes, formatRelative } from "../lib/format";
  import type { BackupResultView, Destination, GameView } from "../lib/types";
  import { listen } from "@tauri-apps/api/event";
  import type { AutoStatusView } from "../lib/types";
  import GameRow from "../components/GameRow.svelte";
  import DestinationPicker from "../components/DestinationPicker.svelte";
  import StatusAutomatico from "../components/StatusAutomatico.svelte";
  import AvisoDisco from "../components/AvisoDisco.svelte";

  let status = $state<AutoStatusView | null>(null);
  let disco = $state<Destination | null>(null);

  $effect(() => {
    api.autoStatus().then((s) => (status = s)).catch(() => {});
    const parar = [
      listen<AutoStatusView>("status-automatico", (e) => (status = e.payload)),
      listen<Destination>("disco-plugado", (e) => (disco = e.payload)),
      listen<string>("disco-removido", (e) => {
        if (disco?.id === e.payload) disco = null;
      }),
    ];
    return () => parar.forEach((p) => p.then((f) => f()));
  });

  let games = $state<GameView[]>([]);
  let scanned = $state(false);
  let scanning = $state(false);
  let erro = $state<string | null>(null);
  let selected = $state<string[]>([]);
  let mostrarGrandes = $state(false);

  let picker = $state<{ ids: string[]; destinations: Destination[] } | null>(null);
  let copiando = $state(false);
  let resultado = $state<BackupResultView[] | null>(null);

  const normais = $derived(games.filter((g) => !g.grande));
  const grandes = $derived(games.filter((g) => g.grande));
  const emRisco = $derived(normais.filter((g) => g.priority === "alta"));
  const ultimo = $derived(
    games.map((g) => g.last_backup).filter((d): d is string => !!d).sort().at(-1) ?? null,
  );
  const totalBytes = $derived(normais.reduce((s, g) => s + g.total_bytes, 0));

  async function escanear() {
    scanning = true;
    erro = null;
    try {
      games = await api.scan();
      scanned = true;
      selected = selected.filter((id) => games.some((g) => g.id === id));
    } catch (e) {
      erro = errorMessage(e);
    } finally {
      scanning = false;
    }
  }

  function alternar(id: string) {
    selected = selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id];
  }

  async function abrirPicker(ids: string[]) {
    if (ids.length === 0) return;
    try {
      picker = { ids, destinations: await api.listDestinations() };
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  async function adicionarPasta() {
    if (!picker) return;
    try {
      const nova = await api.addCustomFolder();
      if (nova) picker = { ...picker, destinations: await api.listDestinations() };
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  async function copiar(destIds: string[]) {
    if (!picker) return;
    const ids = picker.ids;
    picker = null;
    copiando = true;
    erro = null;
    try {
      resultado = await api.backupNow(ids, destIds);
      await escanear();
    } catch (e) {
      erro = errorMessage(e);
    } finally {
      copiando = false;
    }
  }

  async function confirmar(id: string, keep: boolean) {
    try {
      await api.confirmHeuristic(id, keep);
      if (keep) {
        games = games.map((g) => (g.id === id ? { ...g, confirmado: true } : g));
      } else {
        games = games.filter((g) => g.id !== id);
        selected = selected.filter((s) => s !== id);
      }
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  const nomeDe = (id: string) => games.find((g) => g.id === id)?.name ?? id;
  const criados = $derived(resultado?.filter((r) => r.status === "criado").length ?? 0);
  const iguais = $derived(resultado?.filter((r) => r.status === "sem_mudanca").length ?? 0);
  const falhas = $derived(resultado?.filter((r) => r.status === "erro") ?? []);
</script>

<section class="banner cartao">
  <div>
    <h1>Seus saves, protegidos</h1>
    <p class="muted">
      {#if scanned}
        {normais.length} jogos encontrados · último backup {formatRelative(ultimo)}
      {:else}
        Escaneie o PC para encontrar os saves dos seus jogos.
      {/if}
    </p>
    <StatusAutomatico {status} />
  </div>
  <button type="button" class="botao-primario grande" onclick={escanear} disabled={scanning || copiando}>
    {scanning ? "Escaneando…" : scanned ? "Escanear de novo" : "Escanear PC"}
  </button>
</section>

{#if disco}
  <AvisoDisco {disco} onclose={() => (disco = null)} />
{/if}

{#if erro}
  <p class="erro" role="alert">{erro}</p>
{/if}

{#if copiando}
  <p class="aviso" role="status">Copiando saves…</p>
{/if}

{#if resultado}
  <div class="cartao resultado" role="status">
    <strong>Backup concluído:</strong>
    {criados} {criados === 1 ? "versão nova" : "versões novas"}{#if iguais > 0}, {iguais} sem mudança{/if}.
    {#each falhas as f (f.game_id + f.destination_id)}
      <p class="erro">{nomeDe(f.game_id) || "Destino"}: {f.message}</p>
    {/each}
    <button type="button" class="fechar" aria-label="Fechar" onclick={() => (resultado = null)}>×</button>
  </div>
{/if}

{#if scanned}
  <section class="resumo">
    <div class="cartao">
      <span class="muted">Jogos encontrados</span>
      <strong>{normais.length}</strong>
    </div>
    <div class="cartao risco">
      <span class="muted">Sem cópia fora do PC</span>
      <strong>{emRisco.length}</strong>
    </div>
    <div class="cartao">
      <span class="muted">Tamanho dos saves</span>
      <strong>{formatBytes(totalBytes)}</strong>
    </div>
  </section>

  <div class="barra">
    <h2>Jogos <span class="muted">(ordenados por prioridade)</span></h2>
    <span class="espaco"></span>
    <button type="button" class="botao-secundario" disabled={emRisco.length === 0 || copiando}
      onclick={() => abrirPicker(emRisco.map((g) => g.id))}>
      Fazer backup de todos em risco
    </button>
    <button type="button" class="botao-primario" disabled={selected.length === 0 || copiando}
      onclick={() => abrirPicker(selected)}>
      Fazer backup dos selecionados ({selected.length})
    </button>
  </div>

  {#if normais.length === 0}
    <p class="muted">Nenhum save encontrado. Se algum jogo ficou de fora, ele poderá ser adicionado à mão em breve.</p>
  {/if}

  <ul class="lista">
    {#each normais as g (g.id)}
      <GameRow game={g} selected={selected.includes(g.id)} ontoggle={() => alternar(g.id)}
        onconfirm={(keep) => confirmar(g.id, keep)} />
    {/each}
  </ul>

  {#if grandes.length > 0}
    <details class="grandes" bind:open={mostrarGrandes}>
      <summary>Grandes demais — confirmar ({grandes.length})</summary>
      <p class="muted">Estes passam de 2 GB. Confira se são mesmo saves antes de copiar.</p>
      <ul class="lista">
        {#each grandes as g (g.id)}
          <GameRow game={g} selected={selected.includes(g.id)} ontoggle={() => alternar(g.id)} />
        {/each}
      </ul>
    </details>
  {/if}
{/if}

{#if picker}
  <DestinationPicker destinations={picker.destinations} preselected={[]} onconfirm={copiar}
    oncancel={() => (picker = null)} onaddfolder={adicionarPasta} />
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    background: linear-gradient(120deg, #0f3b2c, var(--superficie) 60%);
  }
  .banner h1 {
    font-size: 1.6em;
    margin-bottom: 4px;
  }
  .grande {
    font-size: 1.05em;
    padding: 12px 28px;
  }
  .resumo {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
    margin: 20px 0;
  }
  .resumo .cartao {
    display: grid;
    gap: 6px;
  }
  .resumo strong {
    font-size: 2em;
    font-family: var(--fonte-titulo);
  }
  .risco strong {
    color: var(--alerta);
  }
  .barra {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 8px 0 14px;
  }
  .barra h2 {
    font-size: 1.2em;
  }
  .barra h2 .muted {
    font-size: 0.7em;
    font-weight: 400;
  }
  .lista {
    display: grid;
    gap: 10px;
    padding: 0;
    margin: 0;
  }
  .grandes {
    margin-top: 20px;
  }
  .grandes summary {
    cursor: pointer;
    color: var(--aviso);
    margin-bottom: 10px;
  }
  .erro {
    color: var(--alerta);
  }
  .aviso {
    color: var(--aviso);
  }
  .resultado {
    margin-top: 16px;
    position: relative;
  }
  .fechar {
    position: absolute;
    top: 8px;
    right: 12px;
    background: none;
    border: 0;
    color: var(--texto-suave);
    font-size: 1.4em;
  }
</style>
