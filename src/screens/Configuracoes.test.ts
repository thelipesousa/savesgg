import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import Configuracoes from "./Configuracoes.svelte";
import AvisoDisco from "../components/AvisoDisco.svelte";
import type { Destination } from "../lib/types";

const onedrive: Destination = {
  id: "sync:c:/users/ana/onedrive",
  label: "OneDrive",
  kind: { tipo: "pasta_sincronizada", provedor: "one_drive" },
  root: "C:/Users/Ana/OneDrive/SavesGG Backups",
  available: true,
  free_bytes: null,
  has_backup: false,
};

function responder(settings: Record<string, unknown>) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_destinations_cmd") return Promise.resolve([onedrive]);
    return Promise.resolve(null);
  });
}

beforeEach(() => invoke.mockReset());

describe("Configurações", () => {
  test("marcar destino salva", async () => {
    responder({ auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10 });
    render(Configuracoes);
    const box = await screen.findByRole("checkbox", { name: /OneDrive/ });
    await fireEvent.click(box);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_auto_destinations", { ids: ["sync:c:/users/ana/onedrive"] }),
    );
  });

  test("aviso quando não há destino automático", async () => {
    responder({ auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10 });
    render(Configuracoes);
    expect(await screen.findByText(/Nenhum destino escolhido/)).toBeTruthy();
  });

  test("alternar iniciar com o Windows", async () => {
    responder({ auto_destinations: ["sync:c:/users/ana/onedrive"], autostart: true, auto_paused: false, keep_versions: 10 });
    render(Configuracoes);
    const box = await screen.findByRole("checkbox", { name: /Iniciar com o Windows/ });
    expect((box as HTMLInputElement).checked).toBe(true);
    await fireEvent.click(box);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_autostart", { enabled: false }));
  });

  test("portable esconde iniciar com o Windows", async () => {
    responder({ auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10, portatil: true });
    render(Configuracoes);
    expect(await screen.findByText(/Versão portable/)).toBeTruthy();
    expect(screen.queryByRole("checkbox", { name: /Iniciar com o Windows/ })).toBeNull();
  });

  test("pausar backup automático", async () => {
    responder({ auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10, theme: "gamer", game_view: "lista", wizard_done: true });
    render(Configuracoes);
    const box = await screen.findByRole("checkbox", { name: /Pausar backup automático/ });
    await fireEvent.click(box);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_paused", { paused: true }));
  });
});

describe("Aparência", () => {
  const base = { auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10, theme: "gamer", game_view: "lista", wizard_done: true };

  test("escolher tema salva e aplica", async () => {
    responder(base);
    render(Configuracoes);
    await fireEvent.click(await screen.findByRole("radio", { name: /Cyberpunk/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_theme", { theme: "cyberpunk" }));
    expect(document.documentElement.dataset.theme).toBe("cyberpunk");
  });

  test("escolher grade salva", async () => {
    responder(base);
    render(Configuracoes);
    await fireEvent.click(await screen.findByRole("radio", { name: /Grade/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_game_view", { view: "grade" }));
  });

  test("tema atual vem marcado", async () => {
    responder({ ...base, theme: "win11" });
    render(Configuracoes);
    expect(((await screen.findByRole("radio", { name: /Windows 11/ })) as HTMLInputElement).checked).toBe(true);
  });
});

describe("AvisoDisco", () => {
  test("copiar chama copy_all_to com o id", async () => {
    invoke.mockResolvedValue([]);
    const pen: Destination = { ...onedrive, id: "disco:abcd1234", label: "PENDRIVE (E:)", kind: { tipo: "disco_removivel" } };
    render(AvisoDisco, { disco: pen, onclose: vi.fn() });
    expect(screen.getByText(/PENDRIVE \(E:\)/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: /Copiar/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("copy_all_to", { destinationId: "disco:abcd1234" }));
  });

  test("disco que já tem backup oferece atualizar", () => {
    const pen: Destination = { ...onedrive, id: "disco:1", label: "HD (F:)", kind: { tipo: "disco_removivel" }, has_backup: true };
    render(AvisoDisco, { disco: pen, onclose: vi.fn() });
    expect(screen.getByRole("button", { name: /Atualizar backup/ })).toBeTruthy();
  });

  test("muda quantas versões guardar", async () => {
    responder({ auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10 });
    render(Configuracoes);
    const campo = await screen.findByRole("spinbutton", { name: /Versões guardadas por jogo/ });
    await fireEvent.input(campo, { target: { value: "20" } });
    await fireEvent.blur(campo);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_keep_versions", { n: 20 }));
  });

  test("descartados: trazer de volta", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return Promise.resolve({ auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10 });
      if (cmd === "list_destinations_cmd") return Promise.resolve([onedrive]);
      if (cmd === "ignored_games") return Promise.resolve([{ id: "pasta-x", name: "Pasta X" }]);
      return Promise.resolve(null);
    });
    render(Configuracoes);
    await fireEvent.click(await screen.findByRole("button", { name: "Trazer de volta Pasta X" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("confirm_heuristic", { gameId: "pasta-x", keep: true }));
  });
});
