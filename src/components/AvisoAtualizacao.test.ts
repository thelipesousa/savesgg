import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";

const instalar = vi.fn();
vi.mock("../lib/atualizacao", () => ({
  instalarEReiniciar: (...a: unknown[]) => instalar(...a),
}));

import AvisoAtualizacao from "./AvisoAtualizacao.svelte";
import type { Update } from "@tauri-apps/plugin-updater";

const atualizacao = { version: "0.2.1", body: "Correções" } as unknown as Update;

beforeEach(() => {
  instalar.mockReset();
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
