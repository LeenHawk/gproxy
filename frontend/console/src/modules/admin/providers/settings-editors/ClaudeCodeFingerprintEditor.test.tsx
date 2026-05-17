// @vitest-environment jsdom

import { act } from "react";
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import { ClaudeCodeFingerprintEditor } from "./ClaudeCodeFingerprintEditor";

// @ts-expect-error React's test-only act flag is intentionally set on globalThis.
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

const LABELS: Record<string, string> = {
  "common.show": "Show",
  "providers.routing.collapse": "Collapse",
  "providers.fingerprint.title": "Client Fingerprint",
  "providers.fingerprint.summary": "Claude Code {cli} / SDK {sdk}",
  "providers.fingerprint.resetDefault": "Reset Default",
  "providers.fingerprint.invalid": "Invalid fingerprint JSON",
  "providers.fingerprint.hint": "Fingerprint hint",
  "providers.fingerprint.field.cli_version": "Claude Code Version",
  "providers.fingerprint.field.user_type": "User Type",
  "providers.fingerprint.field.entrypoint": "Entrypoint",
  "providers.fingerprint.field.stainless_lang": "Stainless Language",
  "providers.fingerprint.field.stainless_package_version": "Stainless SDK Version",
  "providers.fingerprint.field.stainless_runtime": "Runtime",
  "providers.fingerprint.field.stainless_runtime_version": "Runtime Version",
  "providers.fingerprint.field.stainless_os": "OS",
  "providers.fingerprint.field.stainless_arch": "Architecture",
  "providers.fingerprint.field.stainless_timeout": "Timeout Seconds",
};

function translate(key: string, params?: Record<string, string | number>) {
  let text = LABELS[key] ?? key;
  for (const [name, value] of Object.entries(params ?? {})) {
    text = text.replace(`{${name}}`, String(value));
  }
  return text;
}

function getButtonByText(container: HTMLElement, text: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find(
    (item): item is HTMLButtonElement => item.textContent?.trim() === text,
  );
  if (!button) {
    throw new Error(`button not found: ${text}`);
  }
  return button;
}

function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")?.set;
  if (!setter) {
    throw new Error("HTMLInputElement value setter not found");
  }
  setter.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function Harness() {
  const [value, setValue] = useState(
    JSON.stringify({
      cli_version: "2.1.112",
      stainless_package_version: "0.81.0",
    }),
  );
  return (
    <>
      <ClaudeCodeFingerprintEditor value={value} onChange={setValue} t={translate} />
      <pre data-testid="value">{value}</pre>
    </>
  );
}

describe("ClaudeCodeFingerprintEditor", () => {
  it("renders localized fingerprint labels and edits structured json", async () => {
    const container = document.createElement("div");
    const root = createRoot(container);

    try {
      await act(async () => {
        root.render(React.createElement(Harness));
      });

      expect(container.textContent).toContain("Client Fingerprint");
      expect(container.textContent).toContain("Claude Code 2.1.112 / SDK 0.81.0");

      await act(async () => {
        getButtonByText(container, "Show").click();
      });

      const input = Array.from(container.querySelectorAll("input")).find(
        (item) => item.previousElementSibling?.textContent === "Claude Code Version",
      );
      if (!input) {
        throw new Error("cli version input not found");
      }

      await act(async () => {
        setInputValue(input, "2.2.0");
      });

      expect(container.querySelector("[data-testid='value']")?.textContent).toContain(
        '"cli_version": "2.2.0"',
      );
    } finally {
      await act(async () => {
        root.unmount();
      });
    }
  });
});
