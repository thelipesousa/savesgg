import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, test, vi } from "vitest";
import Capa from "./Capa.svelte";
import GameCard from "./GameCard.svelte";
import type { GameView } from "../lib/types";

function jogo(extra: Partial<GameView> = {}): GameView {
  return {
    id: "elden-ring",
    name: "Elden Ring",
    store: "steam",
    source: "manifesto",
    priority: "alta",
    protections: [],
    files: 2,
    total_bytes: 2048,
    last_backup: null,
    save_roots: ["<winAppData>/EldenRing"],
    grande: false,
    confirmado: false,
    cover_url: "https://cdn.cloudflare.steamstatic.com/steam/apps/1245620/library_600x900.jpg",
    ...extra,
  };
}

describe("Capa", () => {
  test("com URL mostra a imagem", () => {
    const { container } = render(Capa, { game: jogo() });
    expect(container.querySelector("img")?.getAttribute("src")).toContain("/1245620/");
  });

  test("imagem que falha vira a inicial", async () => {
    const { container } = render(Capa, { game: jogo() });
    await fireEvent.error(container.querySelector("img")!);
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("E")).toBeTruthy();
  });

  test("sem AppID mostra a inicial", () => {
    const { container } = render(Capa, { game: jogo({ cover_url: null, name: "Skate" }) });
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("S")).toBeTruthy();
  });
});

describe("GameCard", () => {
  test("mostra nome, prioridade e seleção", async () => {
    const ontoggle = vi.fn();
    render(GameCard, { game: jogo(), selected: false, ontoggle });
    expect(screen.getByText("Elden Ring")).toBeTruthy();
    expect(screen.getByText(/Prioridade alta/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("checkbox", { name: "Selecionar Elden Ring" }));
    expect(ontoggle).toHaveBeenCalled();
  });
});
