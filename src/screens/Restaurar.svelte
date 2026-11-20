<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import { formatBytes, formatDateTime } from "../lib/format";
  import type { Destination, GameSnapshotsView, RestorePlan, RestoreReport } from "../lib/types";

  let destinos = $state<Destination[]>([]);
  let destinoId = $state<string | null>(null);
  let jogos = $state<GameSnapshotsView[]>([]);
  let plano = $state<RestorePlan | null>(null);
  let pastas = $state<Record<string, string>>({});
  let relatorio = $state<RestoreReport | null>(null);
  let carregando = $state(false);
  let erro = $state<string | null>(null);

  async function carregarDestinos() {
    try {
      destinos = (await api.listDestinations()).filter((d) => d.available && d.has_backup);
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  $effect(() => {
    carregarDestinos();
  });

  async function escolherDestino(id: string) {
    destinoId = id;
    plano = null;
    relatorio = null;
    carregando = true;
    erro = null;
    try {
      jogos = await api.listSnapshots(id);
    } catch (e) {
      erro = errorMessage(e);
    } finally {
      carregando = false;
    }
  }

  async function prepararRestauracao(gameId: string, stem: string) {
    if (!destinoId) return;
    erro = null;
    relatorio = null;
    pastas = {};
    try {
      plano = await api.planRestore(destinoId, gameId, stem);
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  async function escolherPasta(zipPath: string) {
    const p = await api.pickFolder();
    if (p) pastas = { ...pastas, [zipPath]: p };
  }

  const faltando = $derived(plano ? plano.items.filter((i) => !i.target && !pastas[i.zip_path]).length : 0);

  async function restaurar() {
    if (!plano || !destinoId) return;
    carregando = true;
    erro = null;
    try {
      relatorio = await api.applyRestore(destinoId, plano.game_id, plano.stem, pastas);
      plano = null;
      jogos = await api.listSnapshots(destinoId);
    } catch (e) {
      erro = errorMessage(e);
    } finally {
      carregando = false;
    }
  }
</script>

<h1>Restaurar saves</h1>
<p class="muted">Escolha onde está o backup. Cada save volta para o lugar certo neste PC.</p>

{#if erro}<p class="erro" role="alert">{erro}</p>{/if}

{#if relatorio}
  <div class="cartao ok" role="status">
    {relatorio.restored} {relatorio.restored === 1 ? "arquivo restaurado" : "arquivos restaurados"}.
    {#if relatorio.safety_snapshot}
      Os saves que estavam aqui foram guardados antes, numa versão fixada ({relatorio.safety_snapshot.stem}).
    {/if}
  </div>
{/if}

<section class="destinos">
  {#if destinos.length === 0}
    <p class="muted">Nenhum backup encontrado. Plugue o pendrive/HD ou confira a pasta da nuvem.</p>
    <button type="button" class="botao-secundario" onclick={carregarDestinos}>Procurar de novo</button>
  {/if}
  {#each destinos as d (d.id)}
    <button type="button" class="cartao destino" class:ativo={d.id === destinoId} onclick={() => escolherDestino(d.id)}>
      <strong>{d.label}</strong>
      <span class="muted mono">{d.root}</span>
    </button>
  {/each}
</section>

{#if carregando}<p class="muted" role="status">Carregando…</p>{/if}

{#if destinoId && !plano}
  <ul class="jogos">
    {#each jogos as j (j.game_id)}
      <li class="cartao">
        <h3>{j.game_name}</h3>
        <ul class="versoes">
          {#each j.snapshots as s, i (s.stem)}
            <li>
              <span>{formatDateTime(s.created)}</span>
              {#if i === 0}<span class="chip ok">mais recente</span>{/if}
              {#if s.pinned}<span class="chip" title="Versão fixada: nunca é apagada">📌 fixada</span>{/if}
              <span class="muted">{formatBytes(s.total_bytes)}</span>
              <span class="espaco"></span>
              <button type="button" class="botao-secundario" onclick={() => prepararRestauracao(j.game_id, s.stem)}>
                Restaurar
              </button>
            </li>
          {/each}
        </ul>
      </li>
    {/each}
  </ul>
{/if}

{#if plano}
  <section class="cartao plano">
    <h2>Restaurar {plano.game_name}</h2>
    <p class="muted mono">Versão {plano.stem}</p>
    {#if plano.conflicts > 0}
      <p class="aviso">
        {plano.conflicts} {plano.conflicts === 1 ? "arquivo já existe" : "arquivos já existem"} neste PC.
        Eles serão guardados numa versão fixada antes de serem substituídos.
      </p>
    {/if}
    <ul class="itens">
      {#each plano.items as item (item.zip_path)}
        <li>
          <span class="mono">{item.template}</span>
          {#if item.target}
            <span class="chip ok">{item.exists ? "vai substituir" : "pronto"}</span>
          {:else if pastas[item.zip_path]}
            <span class="chip ok mono">{pastas[item.zip_path]}</span>
          {:else}
            <button type="button" class="botao-secundario" onclick={() => escolherPasta(item.zip_path)}>Escolher pasta…</button>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="acoes">
      <button type="button" class="botao-secundario" onclick={() => (plano = null)}>Voltar</button>
      <span class="espaco"></span>
      {#if faltando > 0}<span class="muted">{faltando} sem pasta (serão pulados)</span>{/if}
      <button type="button" class="botao-primario" onclick={restaurar} disabled={carregando}>Restaurar agora</button>
    </div>
  </section>
{/if}

<style>
  h1 {
    margin-bottom: 4px;
  }
  .destinos {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin: 20px 0;
  }
  .destino {
    display: grid;
    gap: 4px;
    text-align: left;
    color: var(--texto);
    min-width: 260px;
  }
  .destino.ativo {
    border-color: var(--destaque);
  }
  .jogos,
  .versoes,
  .itens {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 10px;
  }
  .versoes {
    margin-top: 10px;
    gap: 6px;
  }
  .versoes li,
  .itens li {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .versoes button {
    padding: 4px 14px;
  }
  .plano {
    display: grid;
    gap: 12px;
  }
  .erro {
    color: var(--alerta);
  }
  .aviso {
    color: var(--aviso);
  }
  .ok {
    border-color: rgb(25 230 160 / 0.4);
  }
</style>
