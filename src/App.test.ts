import { render, screen } from "@testing-library/svelte";
import { expect, test, vi } from "vitest";
import App from "./App.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => Promise.resolve([])) }));

test("mostra o nome do app e o menu", () => {
  render(App);
  expect(screen.getByRole("heading", { name: /SaveGuard/ })).toBeTruthy();
  expect(screen.getByRole("button", { name: /Escanear PC/ })).toBeTruthy();
  expect(screen.getByRole("navigation", { name: "Menu principal" })).toBeTruthy();
});
