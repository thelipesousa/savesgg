import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const procurar = vi.fn();
vi.mock("../lib/atualizacao", () => ({
  procurarAtualizacao: () => procurar(),
  instalarEReiniciar: vi.fn(),
}));

import Sobre from "./Sobre.svelte";

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue("0.2.0");
});

test("abre a pasta de logs", async () => {
  render(Sobre);
  await fireEvent.click(screen.getByRole("button", { name: "Abrir pasta de logs" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_logs_folder"));
});

test("procurar atualização sem versão nova avisa que está em dia", async () => {
  procurar.mockResolvedValue({ nova: null, erro: null });
  render(Sobre);
  await fireEvent.click(screen.getByRole("button", { name: "Procurar atualização" }));
  expect(await screen.findByText("Você está na versão mais recente.")).toBeTruthy();
});

test("procurar atualização com versão nova mostra o aviso", async () => {
  procurar.mockResolvedValue({ nova: { version: "0.2.1", body: "" }, erro: null });
  render(Sobre);
  await fireEvent.click(screen.getByRole("button", { name: "Procurar atualização" }));
  expect(await screen.findByText("Versão 0.2.1 disponível")).toBeTruthy();
});

test("falha ao procurar atualização não diz que está em dia", async () => {
  procurar.mockResolvedValue({ nova: null, erro: "sem internet" });
  render(Sobre);
  await fireEvent.click(screen.getByRole("button", { name: "Procurar atualização" }));
  expect(await screen.findByText(/Não deu para procurar atualização/)).toBeTruthy();
  expect(screen.queryByText("Você está na versão mais recente.")).toBeNull();
});
