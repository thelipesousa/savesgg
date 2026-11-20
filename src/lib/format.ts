const num = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1 });
const rel = new Intl.RelativeTimeFormat("pt-BR", { numeric: "auto" });

/** Tamanho em unidades decimais: 18400000 → "18,4 MB". */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${num.format(v)} ${units[i]}`;
}

/** Tempo relativo em português: "há 4 minutos", "ontem", "nunca". */
export function formatRelative(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "nunca";
  const diff = (new Date(iso).getTime() - now.getTime()) / 1000;
  const abs = Math.abs(diff);
  if (abs < 60) return rel.format(Math.round(diff), "second");
  if (abs < 3600) return rel.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rel.format(Math.round(diff / 3600), "hour");
  return rel.format(Math.round(diff / 86400), "day");
}

/** Data e hora curtas: "20/11/2026 21:40". */
export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" });
}
