import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import Assistente from "./Assistente.svelte";
import App from "../App.svelte";

const jogo = (id: string, priority: string) => ({
  id, name: id, store: "steam", source: "manifesto", priority, protections: [], files: 1,
  total_bytes: 1, last_backup: null, save_roots: [], grande: false, confirmado: false, cover_url: null,
});
const onedrive = {
  id: "sync:c:/users/ana/onedrive", label: "OneDrive", kind: { tipo: "pasta_sincronizada", provedor: "one_drive" },
  root: "C:/Users/Ana/OneDrive/SaveGuard Backups", available: true, free_bytes: null, has_backup: false,
};

function responder(dests: unknown[] = [onedrive], wizardDone = false) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "scan")
      return Promise.resolve([jogo("a", "baixa"), jogo("b", "alta"), jogo("c", "alta"), jogo("d", "media")]);
    if (cmd === "list_destinations_cmd") return Promise.resolve(dests);
    if (cmd === "get_settings")
      return Promise.resolve({ auto_destinations: [], autostart: true, auto_paused: false, keep_versions: 10, theme: "gamer", game_view: "lista", wizard_done: wizardDone });
    return Promise.resolve(null);
  });
}

beforeEach(() => invoke.mockReset());

describe("Assistente", () => {
  test("fluxo completo", async () => {
    responder();
    const onfinish = vi.fn();
    render(Assistente, { onfinish });
    await fireEvent.click(screen.getByRole("button", { name: /Começar/ }));
    expect(await screen.findByText(/1 já protegido/)).toBeTruthy();
    expect(screen.getByText(/2 sem cópia fora do PC/)).toBeTruthy();
    expect(screen.getByText(/1 com proteção incerta/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: /Continuar/ }));
    await fireEvent.click(await screen.findByRole("checkbox", { name: /OneDrive/ }));
    await fireEvent.click(screen.getByRole("button", { name: /Continuar/ }));
    await fireEvent.click(await screen.findByRole("radio", { name: /Pastel/ }));
    await fireEvent.click(screen.getByRole("button", { name: /Concluir/ }));
    await waitFor(() => expect(onfinish).toHaveBeenCalled());
    const cmds = invoke.mock.calls.map((c) => c[0]);
    expect(cmds.indexOf("scan")).toBeLessThan(cmds.indexOf("set_auto_destinations"));
    expect(cmds.indexOf("set_auto_destinations")).toBeLessThan(cmds.indexOf("set_theme"));
    expect(cmds.indexOf("set_theme")).toBeLessThan(cmds.indexOf("complete_wizard"));
    expect(invoke).toHaveBeenCalledWith("set_auto_destinations", { ids: ["sync:c:/users/ana/onedrive"] });
    expect(invoke).toHaveBeenCalledWith("set_theme", { theme: "pastel" });
  });

  test("mantém os destinos de quem já tinha configurado", async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "scan") return Promise.resolve([jogo("a", "alta")]);
      if (cmd === "list_destinations_cmd") return Promise.resolve([onedrive]);
      if (cmd === "get_settings")
        return Promise.resolve({ auto_destinations: ["sync:c:/users/ana/onedrive", "disco:00000001"], autostart: true,
          auto_paused: false, keep_versions: 10, theme: "gamer", game_view: "lista", wizard_done: false });
      return Promise.resolve(null);
    });
    render(Assistente, { onfinish: vi.fn() });
    await fireEvent.click(screen.getByRole("button", { name: /Começar/ }));
    await screen.findByText(/sem cópia fora do PC/);
    await fireEvent.click(screen.getByRole("button", { name: /Continuar/ }));
    expect(((await screen.findByRole("checkbox", { name: /OneDrive/ })) as HTMLInputElement).checked).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: /Continuar/ }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_auto_destinations", { ids: ["sync:c:/users/ana/onedrive", "disco:00000001"] }),
    );
  });

  test("sem destino mostra aviso e deixa seguir", async () => {
    responder([]);
    render(Assistente, { onfinish: vi.fn() });
    await fireEvent.click(screen.getByRole("button", { name: /Começar/ }));
    await screen.findByText(/1 já protegido/);
    await fireEvent.click(screen.getByRole("button", { name: /Continuar/ }));
    expect(await screen.findByText(/Nenhum destino marcado/)).toBeTruthy();
    await waitFor(() =>
      expect((screen.getByRole("button", { name: /Continuar/ }) as HTMLButtonElement).disabled).toBe(false),
    );
  });

  test("pular conclui", async () => {
    responder();
    const onfinish = vi.fn();
    render(Assistente, { onfinish });
    await fireEvent.click(screen.getByRole("button", { name: /Pular configuração/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("complete_wizard"));
    expect(onfinish).toHaveBeenCalled();
  });
});

describe("App", () => {
  test("mostra o assistente quando não foi concluído", async () => {
    responder([onedrive], false);
    render(App);
    expect(await screen.findByRole("heading", { name: /Bem-vindo ao SaveGuard/ })).toBeTruthy();
  });

  test("mostra o menu quando já foi concluído", async () => {
    responder([onedrive], true);
    render(App);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_settings"));
    expect(screen.getByRole("navigation", { name: "Menu principal" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: /Bem-vindo ao SaveGuard/ })).toBeNull();
  });
});
