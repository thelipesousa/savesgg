// Atualização automática: procura a versão nova no repositório público de lançamentos,
// baixa, confere a assinatura (o plugin recusa arquivo sem assinatura válida) e reinicia.
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** Versão nova disponível, ou `null` (inclusive quando não deu para procurar). */
export async function procurarAtualizacao(): Promise<Update | null> {
  try {
    return await check();
  } catch (e) {
    console.warn("não deu para procurar atualização", e);
    return null;
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
