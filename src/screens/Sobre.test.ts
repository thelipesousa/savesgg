import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

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
