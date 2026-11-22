import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import ContasNuvem from "./ContasNuvem.svelte";
import type { Destination, StatusNuvem } from "../lib/types";

const conta: Destination = {
  id: "nuvem:gdrive:abc",
  label: "Google Drive (ana@gmail.com)",
  kind: { tipo: "nuvem", provedor: "gdrive" },
  root: "C:/x",
  available: true,
  free_bytes: null,
  has_backup: false,
};

function status(extra: Partial<StatusNuvem> = {}): StatusNuvem {
  return { destination_id: conta.id, pendentes: 0, enviando: false, erro: null, precisa_entrar: false, ...extra };
}

function responder(disponivel: boolean) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "login_nuvem_disponivel") return Promise.resolve({ gdrive: disponivel });
    if (cmd === "entrar_nuvem") return new Promise(() => {});
    return Promise.resolve(null);
  });
}

beforeEach(() => {
  invoke.mockReset();
});

test("botão desabilitado quando o build não tem login", async () => {
  responder(false);
  render(ContasNuvem, { destinos: [], status: [], onmudou: vi.fn() });
  expect(await screen.findByText("Login na nuvem indisponível nesta versão")).toBeTruthy();
  expect((screen.getByRole("button", { name: "Entrar com Google (Drive)" }) as HTMLButtonElement).disabled).toBe(true);
});

test("entrar mostra a espera e chama a API", async () => {
  responder(true);
  render(ContasNuvem, { destinos: [], status: [], onmudou: vi.fn() });
  const botao = await screen.findByRole("button", { name: "Entrar com Google (Drive)" });
  await waitFor(() => expect((botao as HTMLButtonElement).disabled).toBe(false));
  await fireEvent.click(botao);
  expect(invoke).toHaveBeenCalledWith("entrar_nuvem", { provedor: "gdrive" });
  expect(await screen.findByText("Termine o login no navegador…")).toBeTruthy();
});

test("conta mostra o estado do envio", async () => {
  responder(true);
  const { rerender } = render(ContasNuvem, { destinos: [conta], status: [status({ pendentes: 3 })], onmudou: vi.fn() });
  expect(await screen.findByText("3 versões esperando envio")).toBeTruthy();
  await rerender({ destinos: [conta], status: [status({ precisa_entrar: true, erro: "entre de novo na conta" })], onmudou: vi.fn() });
  expect(screen.getByText("Entre de novo na conta para continuar enviando.")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Entrar de novo" })).toBeTruthy();
  await rerender({ destinos: [conta], status: [status()], onmudou: vi.fn() });
  expect(screen.getByText("Tudo enviado")).toBeTruthy();
});

test("sair pede confirmação e chama a API", async () => {
  responder(true);
  const onmudou = vi.fn();
  vi.spyOn(window, "confirm").mockReturnValue(true);
  render(ContasNuvem, { destinos: [conta], status: [status()], onmudou });
  await fireEvent.click(await screen.findByRole("button", { name: "Sair de ana@gmail.com" }));
  expect(window.confirm).toHaveBeenCalledWith("Sair de ana@gmail.com? Os backups na nuvem continuam lá.");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("sair_nuvem", { destinationId: conta.id }));
  await waitFor(() => expect(onmudou).toHaveBeenCalled());
});

test("enviar agora chama a API", async () => {
  responder(true);
  render(ContasNuvem, { destinos: [conta], status: [status({ pendentes: 1 })], onmudou: vi.fn() });
  await fireEvent.click(await screen.findByRole("button", { name: "Enviar agora" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("enviar_agora"));
});

test("cancelar para a espera do login", async () => {
  responder(true);
  render(ContasNuvem, { destinos: [], status: [], onmudou: vi.fn() });
  const botao = await screen.findByRole("button", { name: "Entrar com Google (Drive)" });
  await waitFor(() => expect((botao as HTMLButtonElement).disabled).toBe(false));
  await fireEvent.click(botao);
  await fireEvent.click(await screen.findByRole("button", { name: "Cancelar" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("cancelar_login"));
});
