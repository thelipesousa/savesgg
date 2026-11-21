<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import { formatBytes, formatDateTime } from "../lib/format";
  import { listen } from "@tauri-apps/api/event";
  import type { Destination, GameSnapshotsView, LoteItemView, RestorePlan, RestoreReport } from "../lib/types";

  let destinos = $state<Destination[]>([]);
  let destinoId = $state<string | null>(null);
  let jogos = $state<GameSnapshotsView[]>([]);
  let plano = $state<RestorePlan | null>(null);
  let pastas = $state<Record<string, string>>({});
  let relatorio = $state<RestoreReport | null>(null);
  let carregando = $state(false);
  let erro = $state<string | null>(null);
  /** Jogos marcados para restaurar em lote. */
  let marcados = $state<Set<string>>(new Set());
  let progresso = $state<{ feitos: number; total: number; jogo: string } | null>(null);
  let resumo = $state<LoteItemView[] | null>(null);

  /** Pastas abertas nesta sessão (somem ao recarregar a lista de destinos detectados). */
  let abertas = $state<Destination[]>([]);

  async function carregarDestinos() {
    try {
      const achados = (await api.listDestinations()).filter((d) => d.available && d.has_backup);
      destinos = [...achados, ...abertas.filter((a) => !achados.some((d) => d.id === a.id))];
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  $effect(() => {
    carregarDestinos();
  });

  async function abrirPasta() {
    erro = null;
    try {
      const d = await api.openBackupFolder();
      if (!d) return;
      if (!abertas.some((a) => a.id === d.id)) abertas = [...abertas, d];
      if (!destinos.some((x) => x.id === d.id)) destinos = [...destinos, d];
      await escolherDestino(d.id);
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  async function escolherDestino(id: string) {
    destinoId = id;
    plano = null;
    relatorio = null;
    carregando = true;
    erro = null;
    resumo = null;
    try {
      jogos = await api.listSnapshots(id);
      marcados = new Set(jogos.map((j) => j.game_id));
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

  /** A versão mais recente que não é de segurança (a lista vem da mais nova para a mais antiga). */
  function maisRecente(j: GameSnapshotsView): string | undefined {
    return j.snapshots.find((s) => !s.seguranca)?.stem;
  }

  function alternar(id: string) {
    const novo = new Set(marcados);
    if (novo.has(id)) novo.delete(id);
    else novo.add(id);
    marcados = novo;
  }

  async function restaurarSelecionados() {
    if (!destinoId || marcados.size === 0) return;
    erro = null;
    relatorio = null;
    resumo = null;
    const ids = jogos.map((j) => j.game_id).filter((id) => marcados.has(id));
    progresso = { feitos: 0, total: ids.length, jogo: "" };
    const parar = await listen<{ feitos: number; total: number; jogo: string }>("restauracao-progresso", (ev) => {
      progresso = ev.payload;
    });
    try {
      resumo = await api.restoreMany(destinoId, ids);
      jogos = await api.listSnapshots(destinoId);
    } catch (e) {
      erro = errorMessage(e);
    } finally {
      parar();
      progresso = null;
    }
  }

  async function fixar(gameId: string, stem: string, pinned: boolean) {
    if (!destinoId) return;
    erro = null;
    try {
      await api.setPinned(destinoId, gameId, stem, pinned);
      jogos = await api.listSnapshots(destinoId);
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  async function guardar(gameId: string, valor: string) {
    erro = null;
    try {
      await api.setGameKeep(gameId, valor === "" ? null : Number(valor));
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  const OPCOES_GUARDAR = [3, 5, 10, 20, 50];

  /** Abre o fluxo de um jogo (escolher pasta) na versão que o lote tentou restaurar. */
  function abrirJogo(gameId: string) {
    const j = jogos.find((x) => x.game_id === gameId);
    const stem = j && maisRecente(j);
    if (stem) prepararRestauracao(gameId, stem);
  }

  const restaurados = $derived(resumo?.filter((r) => r.resultado.tipo === "restaurado") ?? []);
  const precisam = $derived(resumo?.filter((r) => r.resultado.tipo === "precisa_de_pasta") ?? []);
  const comErro = $derived(resumo?.filter((r) => r.resultado.tipo === "erro") ?? []);

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
  <button type="button" class="botao-secundario" onclick={abrirPasta}>Procurar backup em outra pasta…</button>
  {#each destinos as d (d.id)}
    <button type="button" class="cartao destino" class:ativo={d.id === destinoId} onclick={() => escolherDestino(d.id)}>
      <strong>{d.label}</strong>
      <span class="muted mono">{d.root}</span>
    </button>
  {/each}
</section>

{#if carregando}<p class="muted" role="status">Carregando…</p>{/if}

{#if progresso}
  <div class="cartao" role="status">
    <p>Restaurando {progresso.jogo || "…"} ({progresso.feitos + 1} de {progresso.total})</p>
    <progress max={progresso.total} value={progresso.feitos}></progress>
  </div>
{/if}

{#if resumo}
  <section class="cartao resumo" aria-label="Resumo da restauração">
    <h2>{restaurados.length} {restaurados.length === 1 ? "restaurado" : "restaurados"}</h2>
    {#if precisam.length > 0}
      <p>Precisam que você escolha a pasta:</p>
      <ul>
        {#each precisam as r (r.game_id)}
          <li>
            {r.game_name}
            <button
              type="button"
              class="botao-secundario"
              aria-label={`Escolher pasta para ${r.game_name}`}
              onclick={() => abrirJogo(r.game_id)}>Escolher pasta…</button
            >
          </li>
        {/each}
      </ul>
    {/if}
    {#if comErro.length > 0}
      <p class="erro">Não foi possível restaurar:</p>
      <ul>
        {#each comErro as r (r.game_id)}
          <li>{r.game_name}: {r.resultado.tipo === "erro" ? r.resultado.mensagem : ""}</li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

{#if destinoId && !plano}
  {#if jogos.length > 0}
    <div class="acoes-lote">
      <button type="button" class="botao-secundario" onclick={() => (marcados = new Set(jogos.map((j) => j.game_id)))}>
        Marcar todos
      </button>
      <button type="button" class="botao-secundario" onclick={() => (marcados = new Set())}>Nenhum</button>
      <span class="espaco"></span>
      <button
        type="button"
        class="botao-primario"
        disabled={marcados.size === 0 || progresso !== null}
        onclick={restaurarSelecionados}>Restaurar selecionados ({marcados.size})</button
      >
    </div>
  {/if}
  <ul class="jogos">
    {#each jogos as j (j.game_id)}
      <li class="cartao">
        <label class="titulo-jogo">
          <input type="checkbox" checked={marcados.has(j.game_id)} onchange={() => alternar(j.game_id)} />
          <h3>{j.game_name}</h3>
        </label>
        <label class="guardar muted">
          Guardar:
          <select
            aria-label={`Versões guardadas de ${j.game_name}`}
            value={j.keep === null || j.keep === undefined ? "" : String(j.keep)}
            onchange={(e) => guardar(j.game_id, (e.currentTarget as HTMLSelectElement).value)}
          >
            <option value="">padrão</option>
            {#each OPCOES_GUARDAR as n (n)}<option value={String(n)}>{n}</option>{/each}
          </select>
        </label>
        <ul class="versoes">
          {#each j.snapshots as s (s.stem)}
            <li>
              <span>{formatDateTime(s.created)}</span>
              {#if s.stem === maisRecente(j)}<span class="chip ok">mais recente</span>{/if}
              {#if s.seguranca}
                <span class="chip" title="Cópia do que estava no PC antes de uma restauração">versão de segurança</span>
              {/if}
              {#if s.pinned}<span class="chip" title="Versão fixada: nunca é apagada">📌 fixada</span>{/if}
              <span class="muted">{formatBytes(s.total_bytes)}</span>
              <span class="espaco"></span>
              <button type="button" class="botao-secundario" onclick={() => fixar(j.game_id, s.stem, !s.pinned)}>
                {s.pinned ? "Desafixar" : "Fixar"}
              </button>
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
  .guardar {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    font-size: 0.9em;
  }
  .acoes-lote {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  .titulo-jogo {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .resumo ul {
    list-style: none;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  progress {
    width: 100%;
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
    border-color: var(--ok);
  }
</style>
