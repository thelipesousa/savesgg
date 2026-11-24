import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";

const instalar = vi.fn();
vi.mock("../lib/atualizacao", () => ({
  instalarEReiniciar: (...a: unknown[]) => instalar(...a),
}));

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import AvisoAtualizacao from "./AvisoAtualizacao.svelte";
import type { Update } from "@tauri-apps/plugin-updater";

const atualizacao = { version: "0.2.1", body: "Correções" } as unknown as Update;

beforeEach(() => {
  instalar.mockReset();
  invoke.mockReset();
  invoke.mockResolvedValue(false);
});

test("mostra a versão e as notas", () => {
  render(AvisoAtualizacao, { atualizacao });
  expect(screen.getByText("Versão 0.2.1 disponível")).toBeTruthy();
  expect(screen.getByText("Correções")).toBeTruthy();
});

test("atualizar chama instalar e mostra a porcentagem", async () => {
  instalar.mockImplementation((_u: Update, aoProgredir: (pct: number) => void) => {
    aoProgredir(40);
    return new Promise(() => {});
  });
  render(AvisoAtualizacao, { atualizacao });
  await fireEvent.click(screen.getByRole("button", { name: "Atualizar e reiniciar" }));
  expect(instalar).toHaveBeenCalled();
  await waitFor(() => expect(screen.getByRole("button", { name: /40%/ })).toBeTruthy());
  expect((screen.getByRole("button", { name: /40%/ }) as HTMLButtonElement).disabled).toBe(true);
});

test("no portable abre a página de downloads em vez de instalar", async () => {
  invoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "modo_portatil" ? true : null));
  render(AvisoAtualizacao, { atualizacao });
  const botao = await screen.findByRole("button", { name: "Baixar a versão nova" });
  await fireEvent.click(botao);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("abrir_pagina_download"));
  expect(instalar).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: "Atualizar e reiniciar" })).toBeNull();
});
