<script lang="ts">
  import AvisoNuvem from "../components/AvisoNuvem.svelte";
  import ContasNuvem from "../components/ContasNuvem.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, errorMessage } from "../lib/api";
  import { formatBytes } from "../lib/format";
  import type { AutoStatusView, Destination, IgnoredView, SettingsView, StatusNuvem } from "../lib/types";
  import { TEMAS, aplicarTema } from "../lib/tema";

  let settings = $state<SettingsView | null>(null);
  let destinos = $state<Destination[]>([]);
  let erro = $state<string | null>(null);

  let descartados = $state<IgnoredView[]>([]);
  let statusNuvem = $state<StatusNuvem[]>([]);

  $effect(() => {
    api
      .autoStatus()
      .then((s) => (statusNuvem = s?.nuvem ?? []))
      .catch(() => {});
    const parar = listen<AutoStatusView>("status-automatico", (ev) => {
      statusNuvem = ev.payload.nuvem ?? [];
    });
    return () => {
      parar.then((f) => f()).catch(() => {});
    };
  });

  async function carregar() {
    try {
      [settings, destinos] = await Promise.all([api.getSettings(), api.listDestinations()]);
      descartados = (await api.ignoredGames()) ?? [];
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  $effect(() => {
    carregar();
  });

  async function salvar(acao: () => Promise<void>) {
    erro = null;
    try {
      await acao();
    } catch (e) {
      erro = errorMessage(e);
      await carregar();
    }
  }

  function alternarDestino(id: string) {
    if (!settings) return;
    const atual = settings.auto_destinations;
    const ids = atual.includes(id) ? atual.filter((x) => x !== id) : [...atual, id];
    settings = { ...settings, auto_destinations: ids };
    salvar(() => api.setAutoDestinations(ids));
  }

  function alternarAutostart() {
    if (!settings) return;
    const enabled = !settings.autostart;
    settings = { ...settings, autostart: enabled };
    salvar(() => api.setAutostart(enabled));
  }

  function alternarPausa() {
    if (!settings) return;
    const paused = !settings.auto_paused;
    settings = { ...settings, auto_paused: paused };
    salvar(() => api.setPaused(paused));
  }

  function escolherTema(id: string) {
    if (!settings) return;
    settings = { ...settings, theme: aplicarTema(id) };
    salvar(() => api.setTheme(id));
  }

  function escolherExibicao(view: "lista" | "grade") {
    if (!settings) return;
    settings = { ...settings, game_view: view };
    salvar(() => api.setGameView(view));
  }

  // Destinos escolhidos que não aparecem agora (ex.: pendrive desplugado).
  const ausentes = $derived(
    settings ? settings.auto_destinations.filter((id) => !destinos.some((d) => d.id === id)) : [],
  );

  function icone(d: Destination): string {
    if (d.kind.tipo === "pasta_sincronizada" || d.kind.tipo === "nuvem") return "☁️";
    if (d.kind.tipo === "disco_removivel") return "💾";
    return "📁";
  }

  let erroVersoes = $state<string | null>(null);

  async function trazerDeVolta(id: string) {
    await salvar(async () => {
      await api.confirmHeuristic(id, true);
      descartados = descartados.filter((d) => d.id !== id);
    });
  }

  async function salvarVersoes(valor: string) {
    if (!settings) return;
    const n = Number(valor);
    erroVersoes = null;
    if (n === settings.keep_versions) return;
    try {
      await api.setKeepVersions(n);
      settings = { ...settings, keep_versions: n };
    } catch (e) {
      erroVersoes = errorMessage(e);
    }
  }
</script>

<h1>Configurações</h1>

{#if erro}<p class="erro" role="alert">{erro}</p>{/if}

{#if settings}
  <section class="cartao bloco">
    <h2>Contas na nuvem</h2>
    <p class="muted">Entre na sua conta e os backups sobem direto para o seu Google Drive, na pasta "SavesGG Backups".</p>
    <ContasNuvem destinos={destinos} status={statusNuvem} onmudou={carregar} />
  </section>

  <section class="cartao bloco">
    <h2>Destinos do backup automático</h2>
    <p class="muted">Quando você fecha um jogo, os saves vão sozinhos para cá.</p>
    {#if settings.auto_destinations.length === 0}
      <p class="aviso" role="status">Nenhum destino escolhido: o backup automático não vai copiar nada.</p>
    {/if}
    <ul class="lista-destinos">
      {#each destinos as d (d.id)}
        <li class:indisponivel={!d.available}>
          <label>
            <input
              type="checkbox"
              checked={settings.auto_destinations.includes(d.id)}
              onchange={() => alternarDestino(d.id)}
              aria-label={d.label}
            />
            <span class="icone" aria-hidden="true">{icone(d)}</span>
            <span class="nome">{d.label}</span>
            <span class="detalhes">
              {#if !d.available}indisponível agora{/if}
              {#if d.free_bytes !== null}{formatBytes(d.free_bytes)} livres{/if}
            </span>
          </label>
          <AvisoNuvem destino={d} />
        </li>
      {/each}
      {#each ausentes as id (id)}
        <li class="indisponivel">
          <label>
            <input type="checkbox" checked onchange={() => alternarDestino(id)} aria-label={`Destino desconectado ${id}`} />
            <span class="icone" aria-hidden="true">💾</span>
            <span class="nome">Desconectado agora</span>
            <span class="detalhes">os backups esperam na fila até ele voltar</span>
          </label>
        </li>
      {/each}
    </ul>
  </section>

  <section class="cartao bloco">
    <h2>Aparência</h2>
    <div class="temas" role="radiogroup" aria-label="Tema">
      {#each TEMAS as t (t.id)}
        <label class="tema" class:atual={settings.theme === t.id}>
          <input type="radio" name="tema" value={t.id} checked={settings.theme === t.id}
            onchange={() => escolherTema(t.id)} aria-label={t.nome} />
          <span class="amostra" aria-hidden="true" style:background={t.amostra[0]}>
            <span style:background={t.amostra[1]}></span>
            <span class="ponto" style:background={t.amostra[2]}></span>
          </span>
          <strong>{t.nome}</strong>
          <span class="muted">{t.descricao}</span>
        </label>
      {/each}
    </div>
    <div class="exibicao" role="radiogroup" aria-label="Exibição dos jogos">
      <span>Jogos em:</span>
      {#each [["lista", "Lista"], ["grade", "Grade"]] as [v, nome] (v)}
        <label class="chip" class:ok={settings.game_view === v}>
          <input type="radio" name="exibicao" value={v} checked={settings.game_view === v}
            onchange={() => escolherExibicao(v as "lista" | "grade")} aria-label={nome} />
          {nome}
        </label>
      {/each}
    </div>
  </section>

  <section class="cartao bloco">
    <h2>Funcionamento</h2>
    <label class="opcao">
      <input type="checkbox" checked={settings.autostart} onchange={alternarAutostart} aria-label="Iniciar com o Windows" />
      <span>
        <strong>Iniciar com o Windows</strong>
        <span class="muted">Abre minimizado na bandeja, sem atrapalhar.</span>
      </span>
    </label>
    <label class="opcao">
      <input type="checkbox" checked={settings.auto_paused} onchange={alternarPausa} aria-label="Pausar backup automático" />
      <span>
        <strong>Pausar backup automático</strong>
        <span class="muted">Os backups ficam agendados e voltam quando você retomar.</span>
      </span>
    </label>
    <label class="versoes-campo">
      Versões guardadas por jogo
      <input
        type="number"
        min="1"
        max="100"
        value={settings.keep_versions}
        onblur={(e) => salvarVersoes((e.currentTarget as HTMLInputElement).value)}
      />
      <span class="muted">(as fixadas nunca são apagadas)</span>
    </label>
    {#if erroVersoes}<p class="erro" role="alert">{erroVersoes}</p>{/if}
  </section>

  <section class="cartao">
    <h2>Possíveis saves descartados</h2>
    {#if descartados.length === 0}
      <p class="muted">Nenhum descartado.</p>
    {:else}
      <ul class="descartados">
        {#each descartados as d (d.id)}
          <li>
            <span>{d.name}</span>
            <button
              type="button"
              class="botao-secundario"
              aria-label={`Trazer de volta ${d.name}`}
              onclick={() => trazerDeVolta(d.id)}>Trazer de volta</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{:else if !erro}
  <p class="muted">Carregando…</p>
{/if}

<style>
  .descartados {
    list-style: none;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  .descartados li {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .versoes-campo {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .versoes-campo input {
    width: 5em;
  }
  h1 {
    margin-bottom: 16px;
  }
  .bloco {
    display: grid;
    gap: 10px;
    max-width: 760px;
    margin-bottom: 16px;
  }
  .bloco h2 {
    font-size: 1.1em;
  }
  .opcao {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    cursor: pointer;
  }
  .opcao span {
    display: grid;
    gap: 2px;
  }
  .temas {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 10px;
  }
  .tema {
    display: grid;
    gap: 4px;
    padding: 10px;
    border: 1px solid var(--borda);
    border-radius: var(--raio-pequeno);
    background: var(--superficie-2);
    cursor: pointer;
  }
  .tema.atual {
    border-color: var(--destaque);
    box-shadow: var(--brilho);
  }
  .tema input,
  .exibicao input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .tema:has(input:focus-visible),
  .exibicao label:has(input:focus-visible) {
    outline: 2px solid var(--destaque);
    outline-offset: 2px;
  }
  .amostra {
    display: flex;
    gap: 6px;
    align-items: center;
    height: 44px;
    border-radius: 8px;
    padding: 8px;
  }
  .amostra span {
    flex: 1;
    height: 100%;
    border-radius: 6px;
  }
  .amostra .ponto {
    flex: 0 0 18px;
    height: 18px;
    border-radius: 50%;
  }
  .exibicao {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .exibicao label {
    cursor: pointer;
    position: relative;
  }
  .erro {
    color: var(--alerta);
  }
  .aviso {
    color: var(--aviso);
  }
</style>
