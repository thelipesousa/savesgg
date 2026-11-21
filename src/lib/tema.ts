export interface Tema {
  id: string;
  nome: string;
  descricao: string;
  /** fundo, superfície, destaque (amostra no seletor) */
  amostra: [string, string, string];
}

export const TEMAS: Tema[] = [
  { id: "gamer", nome: "Gamer", descricao: "Escuro com verde neon", amostra: ["#0b100e", "#131a17", "#19e6a0"] },
  { id: "pastel", nome: "Pastel", descricao: "Claro e fofinho", amostra: ["#f4effc", "#ffffff", "#9b87f5"] },
  { id: "win11", nome: "Windows 11", descricao: "Claro e limpo", amostra: ["#f3f3f3", "#ffffff", "#0067c0"] },
  { id: "cyberpunk", nome: "Cyberpunk", descricao: "Neon ciano e rosa", amostra: ["#07090e", "#0c1220", "#00f0ff"] },
  { id: "console", nome: "Console", descricao: "Capas em destaque", amostra: ["#10131a", "#1d2026", "#dbb8ff"] },
];

/** Aplica o tema no `<html>`; id desconhecido vira o padrão. Devolve o id aplicado. */
export function aplicarTema(id: string): string {
  const tema = TEMAS.find((t) => t.id === id)?.id ?? TEMAS[0].id;
  document.documentElement.dataset.theme = tema;
  return tema;
}
