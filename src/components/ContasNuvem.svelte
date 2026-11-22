<script lang="ts">
  import { api, errorMessage } from "../lib/api";
  import type { Destination, StatusNuvem } from "../lib/types";

  interface Props {
    destinos: Destination[];
    status: StatusNuvem[];
    /** Chamado depois de entrar ou sair, para quem usa recarregar os destinos. */
    onmudou: () => void;
  }
  let { destinos, status, onmudou }: Props = $props();

  let disponivel = $state<boolean | null>(null);
  let entrando = $state(false);
  let erro = $state<string | null>(null);

  $effect(() => {
    api
      .loginNuvemDisponivel()
      .then((d) => (disponivel = !!d?.gdrive))
      .catch(() => (disponivel = false));
  });

  const contas = $derived(destinos.filter((d) => d.kind.tipo === "nuvem"));

  function statusDe(id: string): StatusNuvem | undefined {
    return status.find((s) => s.destination_id === id);
  }

  /** "Google Drive (ana@gmail.com)" → "ana@gmail.com". */
  function emailDe(d: Destination): string {
    return d.label.match(/\(([^)]+)\)\s*$/)?.[1] ?? d.label;
  }

  function textoStatus(s: StatusNuvem | undefined): string {
    if (!s) return "Verificando…";
    if (s.precisa_entrar) return "Entre de novo na conta para continuar enviando.";
    if (s.enviando) return "Enviando…";
    if (s.erro) return `Não deu para enviar: ${s.erro}`;
    if (s.pendentes === 1) return "1 versão esperando envio";
    if (s.pendentes > 1) return `${s.pendentes} versões esperando envio`;
    return "Tudo enviado";
  }

  async function entrar() {
    erro = null;
    entrando = true;
    try {
      await api.entrarNuvem("gdrive");
      onmudou();
    } catch (e) {
      erro = errorMessage(e);
    } finally {
      entrando = false;
    }
  }

  async function cancelar() {
    try {
      await api.cancelarLogin();
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  async function sair(d: Destination) {
    const email = emailDe(d);
    if (!confirm(`Sair de ${email}? Os backups na nuvem continuam lá.`)) return;
    erro = null;
    try {
      await api.sairNuvem(d.id);
      onmudou();
    } catch (e) {
      erro = errorMessage(e);
    }
  }

  async function enviarAgora() {
    try {
      await api.enviarAgora();
    } catch (e) {
      erro = errorMessage(e);
    }
  }
</script>

<div class="contas">
  {#each contas as d (d.id)}
    {@const s = statusDe(d.id)}
    <div class="conta">
      <span class="icone" aria-hidden="true">☁️</span>
      <span class="info">
        <strong>{d.label}</strong>
        <span class:aviso={s?.precisa_entrar || !!s?.erro} class="muted">{textoStatus(s)}</span>
      </span>
      {#if s?.precisa_entrar}
        <button type="button" class="botao-secundario" onclick={entrar} disabled={entrando || !disponivel}>
          Entrar de novo
        </button>
      {:else}
        <button type="button" class="botao-secundario" onclick={enviarAgora} disabled={!s?.pendentes}>
          Enviar agora
        </button>
      {/if}
      <button type="button" class="botao-secundario" aria-label={`Sair de ${emailDe(d)}`} onclick={() => sair(d)}>
        Sair
      </button>
    </div>
  {/each}

  {#if entrando}
    <div class="acoes">
      <p class="muted" role="status">Termine o login no navegador…</p>
      <button type="button" class="botao-secundario" onclick={cancelar}>Cancelar</button>
    </div>
  {/if}
  <div class="acoes">
    <button type="button" class="botao-secundario" onclick={entrar} disabled={entrando || disponivel !== true}>
      Entrar com Google (Drive)
    </button>
  </div>
  {#if disponivel === false}
    <p class="muted">Login na nuvem indisponível nesta versão</p>
  {/if}
  {#if erro}<p class="erro" role="alert">{erro}</p>{/if}
</div>

<style>
  .contas {
    display: grid;
    gap: 10px;
  }
  .conta {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .info {
    display: grid;
    flex: 1;
  }
  .acoes {
    display: flex;
    gap: 10px;
  }
  .aviso {
    color: var(--aviso);
  }
  .erro {
    color: var(--alerta);
  }
</style>
