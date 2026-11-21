import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, test, vi } from "vitest";
import DestinationPicker from "./DestinationPicker.svelte";
import type { Destination } from "../lib/types";

const dests: Destination[] = [
  {
    id: "sync:c:/users/ana/onedrive",
    label: "OneDrive",
    kind: { tipo: "pasta_sincronizada", provedor: "one_drive" },
    root: "C:/Users/Ana/OneDrive/SavesGG Backups",
    available: true,
    free_bytes: null,
    has_backup: false,
  },
  {
    id: "disco:abcd1234",
    label: "PENDRIVE (E:)",
    kind: { tipo: "disco_removivel" },
    root: "E:/",
    available: false,
    free_bytes: 1_000_000,
    has_backup: true,
  },
];

describe("DestinationPicker", () => {
  test("destino indisponível vem desabilitado", () => {
    render(DestinationPicker, { destinations: dests, preselected: [], onconfirm: vi.fn(), oncancel: vi.fn(), onaddfolder: vi.fn() });
    const pen = screen.getByRole("checkbox", { name: /PENDRIVE/ }) as HTMLInputElement;
    expect(pen.disabled).toBe(true);
  });

  test("sem destinos automáticos, nada vem marcado e Copiar fica desabilitado", () => {
    render(DestinationPicker, { destinations: dests, preselected: [], onconfirm: vi.fn(), oncancel: vi.fn(), onaddfolder: vi.fn() });
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes.every((b) => !b.checked)).toBe(true);
    expect((screen.getByRole("button", { name: /Copiar/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  test("marcar um destino habilita Copiar e envia os ids", async () => {
    const onconfirm = vi.fn();
    render(DestinationPicker, { destinations: dests, preselected: [], onconfirm, oncancel: vi.fn(), onaddfolder: vi.fn() });
    await fireEvent.click(screen.getByRole("checkbox", { name: /OneDrive/ }));
    const copiar = screen.getByRole("button", { name: /Copiar/ }) as HTMLButtonElement;
    expect(copiar.disabled).toBe(false);
    await fireEvent.click(copiar);
    expect(onconfirm).toHaveBeenCalledWith(["sync:c:/users/ana/onedrive"]);
  });

  test("destinos pré-selecionados vêm marcados", () => {
    render(DestinationPicker, { destinations: dests, preselected: ["sync:c:/users/ana/onedrive"], onconfirm: vi.fn(), oncancel: vi.fn(), onaddfolder: vi.fn() });
    expect((screen.getByRole("checkbox", { name: /OneDrive/ }) as HTMLInputElement).checked).toBe(true);
  });
});
