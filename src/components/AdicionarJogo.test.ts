import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import AdicionarJogo from "./AdicionarJogo.svelte";

// Chaves: uma função devolvida pelo beforeEach vira limpeza e o vitest a chamaria depois.
beforeEach(() => {
  invoke.mockReset();
});

test("mostra o erro do comando ao adicionar jogo", async () => {
  invoke.mockImplementation(() => Promise.reject("Já existe um jogo com esse nome"));
  render(AdicionarJogo, { onadicionado: vi.fn(), onfechar: vi.fn() });
  await fireEvent.input(screen.getByRole("textbox", { name: "Nome do jogo" }), { target: { value: "Meu Jogo" } });
  await fireEvent.click(screen.getByRole("button", { name: "Escolher pasta do save e adicionar" }));
  expect((await screen.findByRole("alert")).textContent).toContain("Já existe um jogo com esse nome");
  expect(invoke).toHaveBeenCalledWith("add_manual_game", { name: "Meu Jogo" });
});

test("adicionado devolve a nova lista", async () => {
  const lista = [{ id: "meu-jogo" }];
  invoke.mockResolvedValue(lista);
  const onadicionado = vi.fn();
  render(AdicionarJogo, { onadicionado, onfechar: vi.fn() });
  await fireEvent.input(screen.getByRole("textbox", { name: "Nome do jogo" }), { target: { value: "Meu Jogo" } });
  await fireEvent.click(screen.getByRole("button", { name: "Escolher pasta do save e adicionar" }));
  await waitFor(() => expect(onadicionado).toHaveBeenCalledWith(lista));
});
