import { describe, expect, test } from "vitest";
import { formatBytes, formatRelative } from "./format";

describe("formatBytes", () => {
  test("megabytes com vírgula", () => {
    expect(formatBytes(18_400_000)).toBe("18,4 MB");
  });
  test("bytes, KB e GB", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(4_200)).toBe("4,2 KB");
    expect(formatBytes(2_500_000_000)).toBe("2,5 GB");
  });
});

describe("formatRelative", () => {
  test("há 4 minutos", () => {
    const agora = new Date("2026-11-20T21:40:00Z");
    const antes = new Date(agora.getTime() - 4 * 60_000);
    expect(formatRelative(antes.toISOString(), agora)).toBe("há 4 minutos");
  });
  test("ontem e dias", () => {
    const agora = new Date("2026-11-20T12:00:00Z");
    expect(formatRelative("2026-11-19T12:00:00Z", agora)).toBe("ontem");
    expect(formatRelative("2026-11-10T12:00:00Z", agora)).toBe("há 10 dias");
  });
  test("sem data", () => {
    expect(formatRelative(null)).toBe("nunca");
  });
});
