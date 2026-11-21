import { fireEvent, render, screen } from "@testing-library/svelte";
import { expect, test, vi } from "vitest";

import GameRow from "./GameRow.svelte";
import type { GameView } from "../lib/types";

const jogo = (source: string): GameView =>
  ({
    id: "meu-jogo", name: "Meu Jogo", store: "sem_loja", source, priority: "alta", protections: [], files: 1,
    total_bytes: 1, last_backup: null, save_roots: [], grande: false, confirmado: false, cover_url: null,
  }) as unknown as GameView;

test("jogo manual tem Remover, que pede confirmação", async () => {
  const onremove = vi.fn();
  vi.spyOn(window, "confirm").mockReturnValue(true);
  render(GameRow, { game: jogo("manual"), selected: false, ontoggle: vi.fn(), onremove });
  await fireEvent.click(screen.getByRole("button", { name: "Remover" }));
  expect(window.confirm).toHaveBeenCalled();
  expect(onremove).toHaveBeenCalled();
});

test("jogo da loja não tem Remover", () => {
  render(GameRow, { game: jogo("manifesto"), selected: false, ontoggle: vi.fn(), onremove: vi.fn() });
  expect(screen.queryByRole("button", { name: "Remover" })).toBeNull();
});

test("mostra quando foi jogado", () => {
  const g = { ...jogo("manifesto"), last_played: new Date(Date.now() - 5 * 86400_000).toISOString() } as GameView;
  render(GameRow, { game: g, selected: false, ontoggle: vi.fn() });
  expect(screen.getByText(/^Jogado há 5 dias/)).toBeTruthy();
});
