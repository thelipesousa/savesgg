import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

// Mede o contraste (WCAG) dos pares de cor de cada tema direto do CSS.
const css = readFileSync("src/theme/temas.css", "utf-8");

function vars(tema: string): Record<string, string> {
  const bloco = css.split(`[data-theme="${tema}"] {`)[1].split("}")[0];
  return Object.fromEntries([...bloco.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

type RGBA = [number, number, number, number];
function cor(v: string): RGBA {
  const h = v.match(/^#([0-9a-f]{6})$/i);
  if (h) return [0, 2, 4].map((i) => parseInt(h[1].slice(i, i + 2), 16)).concat(1) as RGBA;
  const r = v.match(/rgb\((\d+) (\d+) (\d+)(?: \/ ([\d.]+))?\)/);
  if (r) return [+r[1], +r[2], +r[3], r[4] ? +r[4] : 1];
  throw new Error(`cor não reconhecida: ${v}`);
}
function sobre(c: RGBA, fundo: RGBA): RGBA {
  const a = c[3];
  return [0, 1, 2].map((i) => c[i] * a + fundo[i] * (1 - a)).concat(1) as RGBA;
}
function lum(c: RGBA): number {
  const l = c.slice(0, 3).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
}
function contraste(texto: string, fundo: string, base: string): number {
  const b = cor(base);
  const f = sobre(cor(fundo), b);
  const t = sobre(cor(texto), f);
  const [l1, l2] = [lum(t), lum(f)].sort((a, c) => c - a);
  return (l1 + 0.05) / (l2 + 0.05);
}

const PARES: [string, string, string][] = [
  ["alerta", "alerta-fundo", "selo de prioridade alta"],
  ["aviso", "aviso-fundo", "selo de prioridade média"],
  ["ok", "ok-fundo", "selo de prioridade baixa"],
  ["destaque-texto", "destaque", "texto do botão principal"],
  ["destaque", "destaque-fundo", "item ativo do menu"],
  ["texto-suave", "superficie", "texto secundário"],
  ["texto-suave", "superficie-2", "texto secundário em cartão"],
  ["texto", "fundo", "texto principal"],
];

describe.each(["gamer", "pastel", "win11", "cyberpunk", "console"])("tema %s", (tema) => {
  const v = vars(tema);
  test.each(PARES)("%s sobre %s (%s) tem contraste mínimo 4.5:1", (t, f) => {
    const r = contraste(v[t], v[f], v.superficie);
    expect(r, `${tema}: --${t} sobre --${f} = ${r.toFixed(2)}`).toBeGreaterThanOrEqual(4.5);
  });
});
