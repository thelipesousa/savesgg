<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import { formatBytes } from "../lib/format";
  import type { Destination, SettingsView } from "../lib/types";

  let settings = $state<SettingsView | null>(null);
  let destinos = $state<Destination[]>([]);
  let erro = $state<string | null>(null);

  async function carregar() {
    try {
      [settings, destinos] = await Promise.all([api.getSettings(), api.listDestinations()]);
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

  // Destinos escolhidos que não aparecem agora (ex.: pendrive desplugado).
  const ausentes = $derived(
    settings ? settings.auto_destinations.filter((id) => !destinos.some((d) => d.id === id)) : [],
  );

  function icone(d: Destination): string {
    if (d.kind.tipo === "pasta_sincronizada") return "☁️";
    if (d.kind.tipo === "disco_removivel") return "💾";
    return "📁";
  }
</script>

<h1>Configurações</h1>

{#if erro}<p class="erro" role="alert">{erro}</p>{/if}

{#if settings}
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
    <p class="muted">Versões guardadas por jogo: {settings.keep_versions} (as fixadas nunca são apagadas).</p>
  </section>
{:else if !erro}
  <p class="muted">Carregando…</p>
{/if}

<style>
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
  .erro {
    color: var(--alerta);
  }
  .aviso {
    color: var(--aviso);
  }
</style>
