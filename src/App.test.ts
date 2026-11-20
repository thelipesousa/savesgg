import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import App from "./App.svelte";

test("mostra o nome do app", () => {
  render(App);
  expect(screen.getByRole("heading", { name: "SaveGuard" })).toBeTruthy();
});
