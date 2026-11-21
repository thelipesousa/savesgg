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
});
