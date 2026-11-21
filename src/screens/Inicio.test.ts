import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
const ouvintes: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((nome: string, cb: (e: { payload: unknown }) => void) => {
    ouvintes[nome] = cb;
    return Promise.resolve(() => {});
  }),
}));

import Inicio from "./Inicio.svelte";

const jogo = (id: string, name: string) => ({
  id, name, store: "steam", source: "manifesto", priority: "alta", protections: [], files: 1,
  total_bytes: 1, last_backup: null, save_roots: [], grande: false, confirmado: false, cover_url: null,
});

beforeEach(() => invoke.mockReset());

test("mostra a varredura que o motor já fez ao abrir, sem clicar", async () => {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "cached_scan") return Promise.resolve([jogo("a", "Alfa"), jogo("b", "Bravo")]);
    return Promise.resolve(null);
  });
  render(Inicio);
  expect(await screen.findByText("Alfa")).toBeTruthy();
  expect(screen.getByText("Bravo")).toBeTruthy();
  expect(invoke).not.toHaveBeenCalledWith("scan");
});

test("sem varredura ainda, pede para escanear", async () => {
  invoke.mockResolvedValue(null);
  render(Inicio);
  expect(await screen.findByRole("button", { name: "Escanear PC" })).toBeTruthy();
});

test("atualiza quando o motor termina a varredura depois de abrir", async () => {
  invoke.mockResolvedValue(null);
  render(Inicio);
  await screen.findByRole("button", { name: "Escanear PC" });
  ouvintes["varredura-concluida"]({ payload: [jogo("c", "Charlie")] });
  expect(await screen.findByText("Charlie")).toBeTruthy();
});

test("remover jogo manual usa a lista refeita pelo motor", async () => {
  const manual = { ...jogo("meu-jogo", "Meu Jogo"), source: "manual" };
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "cached_scan") return Promise.resolve([manual, jogo("b", "Bravo")]);
    if (cmd === "remove_manual_game") return Promise.resolve([jogo("b", "Bravo")]);
    return Promise.resolve(null);
  });
  vi.spyOn(window, "confirm").mockReturnValue(true);
  render(Inicio);
  await fireEvent.click(await screen.findByRole("button", { name: "Remover" }));
  await waitFor(() => expect(screen.queryByText("Meu Jogo")).toBeNull());
  expect(invoke).toHaveBeenCalledWith("remove_manual_game", { gameId: "meu-jogo" });
  expect(screen.getByText("Bravo")).toBeTruthy();
});
