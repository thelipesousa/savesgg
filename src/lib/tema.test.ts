import { describe, expect, test } from "vitest";
import { TEMAS, aplicarTema } from "./tema";

describe("temas", () => {
  test("os 5 temas na ordem do app", () => {
    expect(TEMAS.map((t) => t.id)).toEqual(["gamer", "pastel", "win11", "cyberpunk", "console"]);
  });

  test("aplicar tema marca o html", () => {
    aplicarTema("pastel");
    expect(document.documentElement.dataset.theme).toBe("pastel");
    aplicarTema("azul");
    expect(document.documentElement.dataset.theme).toBe("gamer");
  });
});
