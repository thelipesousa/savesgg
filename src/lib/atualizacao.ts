// Atualização automática: procura a versão nova no repositório público de lançamentos,
// baixa, confere a assinatura (o plugin recusa arquivo sem assinatura válida) e reinicia.
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** Resultado da busca: a versão nova (ou `null` se está em dia) ou o erro. */
export interface Busca {
  nova: Update | null;
  erro: string | null;
}

/** Procura versão nova. Uma falha (sem internet, release ainda não publicada) vem em `erro`. */
export async function procurarAtualizacao(): Promise<Busca> {
  try {
    return { nova: await check(), erro: null };
  } catch (e) {
    console.warn("não deu para procurar atualização", e);
    return { nova: null, erro: e instanceof Error ? e.message : String(e) };
  }
}

/** Baixa, instala e reinicia o app. `aoProgredir` recebe de 0 a 100. */
export async function instalarEReiniciar(u: Update, aoProgredir: (pct: number) => void): Promise<void> {
  let total = 0;
  let baixado = 0;
  await u.downloadAndInstall((ev) => {
    if (ev.event === "Started") total = ev.data.contentLength ?? 0;
    if (ev.event === "Progress") {
      baixado += ev.data.chunkLength;
      if (total > 0) aoProgredir(Math.min(100, Math.round((baixado / total) * 100)));
    }
    if (ev.event === "Finished") aoProgredir(100);
  });
  await relaunch();
}
