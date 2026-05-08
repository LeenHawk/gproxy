import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const css = readFileSync(new URL("./index.css", import.meta.url), "utf8");

describe("console styles", () => {
  test("dark theme toasts use a dark green background", () => {
    expect(css).toMatch(/:root\[data-theme="dark"\]\s+\.toast\s*{[^}]*background:\s*#064e3b;/s);
    expect(css).toMatch(/:root\[data-theme="dark"\]\s+\.toast\s*{[^}]*color:\s*#ecfdf5;/s);
  });
});
