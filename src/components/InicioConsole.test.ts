import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { describe, expect, test, vi } from "vitest";
import InicioConsole from "./InicioConsole.svelte";
import type { GameView } from "../lib/types";

const g = (id: string, name: string): GameView => ({
  id, name, store: "steam", source: "manifesto", priority: "alta", protections: [], files: 1,
  total_bytes: 10, last_backup: null, save_roots: [], grande: false, confirmado: false, cover_url: null,
});
const jogos = [g("a", "Alfa"), g("b", "Bravo"), g("c", "Charlie")];

describe("Início console", () => {
  test("carrossel lista os jogos", () => {
    render(InicioConsole, { games: jogos, onbackup: vi.fn() });
    const lista = screen.getByRole("list", { name: "Biblioteca" });
    expect(within(lista).getAllByRole("listitem")).toHaveLength(3);
  });

  test("selecionar no carrossel troca o destaque", async () => {
    render(InicioConsole, { games: jogos, onbackup: vi.fn() });
    await fireEvent.click(screen.getByRole("button", { name: "Bravo" }));
    expect(screen.getByRole("heading", { level: 2, name: "Bravo" })).toBeTruthy();
  });

  test("seta para a direita avança", async () => {
    render(InicioConsole, { games: jogos, onbackup: vi.fn() });
    await fireEvent.keyDown(screen.getByRole("button", { name: "Alfa" }), { key: "ArrowRight" });
    expect(screen.getByRole("heading", { level: 2, name: "Bravo" })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Bravo" }));
  });

  test("backup do jogo em destaque", async () => {
    const onbackup = vi.fn();
    render(InicioConsole, { games: jogos, onbackup });
    await fireEvent.click(screen.getByRole("button", { name: /Fazer backup agora/ }));
    expect(onbackup).toHaveBeenCalledWith("a");
  });
});
