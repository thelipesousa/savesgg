import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import Restaurar from "./Restaurar.svelte";
import type { Destination, GameSnapshotsView } from "../lib/types";

const pendrive: Destination = {
  id: "disco:00000001",
  label: "Pendrive",
  kind: { tipo: "disco_removivel" },
  root: "E:/",
  available: true,
  free_bytes: null,
  has_backup: true,
};

function snap(stem: string, extra: Partial<GameSnapshotsView["snapshots"][number]> = {}) {
  return { stem, created: `2026-11-2${stem.length % 9}T10:00:00Z`, pinned: false, total_bytes: 10, seguranca: false, ...extra };
}

let jogos: GameSnapshotsView[] = [];
const extras: Record<string, (args: Record<string, unknown>) => unknown> = {};

function responder() {
  invoke.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "list_destinations_cmd") return Promise.resolve([pendrive]);
    if (cmd === "list_snapshots") return Promise.resolve(jogos);
    if (extras[cmd]) return Promise.resolve(extras[cmd](args));
    return Promise.resolve(null);
  });
}

async function abrirPendrive() {
  render(Restaurar);
  await fireEvent.click(await screen.findByRole("button", { name: /Pendrive/ }));
}

beforeEach(() => {
  invoke.mockReset();
  for (const k of Object.keys(extras)) delete extras[k];
  jogos = [];
});

describe("Restaurar", () => {
  test("versão de segurança tem selo próprio e não é a mais recente", async () => {
    jogos = [
      {
        game_id: "hades",
        game_name: "Hades",
        snapshots: [
          snap("2026-11-21_10-00-00", { created: "2026-11-21T10:00:00Z", seguranca: true, pinned: true }),
          snap("2026-11-20_10-00-00", { created: "2026-11-20T10:00:00Z" }),
        ],
      },
    ];
    responder();
    await abrirPendrive();
    expect(await screen.findByText("versão de segurança")).toBeTruthy();
    const versoes = screen.getAllByRole("listitem").filter((li) => li.parentElement?.classList.contains("versoes"));
    const itens = versoes.filter((li) => li.textContent?.includes("mais recente"));
    expect(itens).toHaveLength(1);
    expect(itens[0].textContent).not.toContain("versão de segurança");
  });

  test("marca todos e restaura os selecionados", async () => {
    jogos = [
      { game_id: "hades", game_name: "Hades", snapshots: [snap("2026-11-20_10-00-00")] },
      { game_id: "celeste", game_name: "Celeste", snapshots: [snap("2026-11-20_11-00-00")] },
    ];
    extras.restore_many = () => [];
    responder();
    await abrirPendrive();
    const hades = await screen.findByRole("checkbox", { name: /Hades/ });
    const celeste = screen.getByRole("checkbox", { name: /Celeste/ });
    expect((hades as HTMLInputElement).checked).toBe(true);
    expect((celeste as HTMLInputElement).checked).toBe(true);
    expect(screen.getByRole("button", { name: "Restaurar selecionados (2)" })).toBeTruthy();
    await fireEvent.click(celeste);
    await fireEvent.click(screen.getByRole("button", { name: "Restaurar selecionados (1)" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("restore_many", { destinationId: "disco:00000001", gameIds: ["hades"] }),
    );
  });

  test("mostra o resumo por resultado", async () => {
    jogos = [
      { game_id: "hades", game_name: "Hades", snapshots: [snap("2026-11-20_10-00-00")] },
      { game_id: "celeste", game_name: "Celeste", snapshots: [snap("2026-11-20_11-00-00")] },
    ];
    extras.restore_many = () => [
      { game_id: "hades", game_name: "Hades", resultado: { tipo: "restaurado", arquivos: 3, seguranca: null } },
      { game_id: "celeste", game_name: "Celeste", resultado: { tipo: "precisa_de_pasta", faltando: 1 } },
    ];
    extras.plan_restore = () => ({ game_id: "celeste", game_name: "Celeste", stem: "2026-11-20_11-00-00", items: [], conflicts: 0, unresolved: 1 });
    responder();
    await abrirPendrive();
    await fireEvent.click(await screen.findByRole("button", { name: /Restaurar selecionados/ }));
    expect(await screen.findByText("1 restaurado")).toBeTruthy();
    const resumo = screen.getByRole("region", { name: "Resumo da restauração" });
    expect(resumo.textContent).toContain("Celeste");
    await fireEvent.click(screen.getByRole("button", { name: "Escolher pasta para Celeste" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("plan_restore", {
        destinationId: "disco:00000001",
        gameId: "celeste",
        stem: "2026-11-20_11-00-00",
      }),
    );
  });

  test("abre uma pasta avulsa e lista os backups dela", async () => {
    jogos = [{ game_id: "hades", game_name: "Hades", snapshots: [snap("2026-11-20_10-00-00")] }];
    extras.open_backup_folder = () => ({
      id: "aberta:d:/backup antigo",
      label: "Pasta escolhida",
      kind: { tipo: "pasta_local" },
      root: "D:/Backup antigo",
      available: true,
      free_bytes: null,
      has_backup: true,
    });
    responder();
    render(Restaurar);
    await fireEvent.click(await screen.findByRole("button", { name: "Procurar backup em outra pasta…" }));
    expect(await screen.findByText("Pasta escolhida")).toBeTruthy();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("list_snapshots", { destinationId: "aberta:d:/backup antigo" }),
    );
  });

  test("pasta sem backup mostra o erro", async () => {
    responder();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_destinations_cmd") return Promise.resolve([pendrive]);
      if (cmd === "open_backup_folder") return Promise.reject("Nenhum backup do SavesGG nesta pasta");
      return Promise.resolve(null);
    });
    render(Restaurar);
    await fireEvent.click(await screen.findByRole("button", { name: "Procurar backup em outra pasta…" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Nenhum backup do SavesGG nesta pasta");
  });
});
