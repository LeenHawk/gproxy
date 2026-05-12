// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../../app/i18n";
import { RewriteRulesTab } from "./RewriteRulesTab";
import type { ProviderFormState } from "./index";

// @ts-expect-error React's test-only act flag is intentionally set on globalThis.
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

function getButtonByText(container: HTMLElement, text: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find(
    (item): item is HTMLButtonElement => item.textContent?.trim() === text,
  );
  if (!button) {
    throw new Error(`button not found: ${text}`);
  }
  return button;
}

function getButtonContaining(container: HTMLElement, text: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find((item): item is HTMLButtonElement =>
    item.textContent?.includes(text) ?? false,
  );
  if (!button) {
    throw new Error(`button containing text not found: ${text}`);
  }
  return button;
}

const initialRules = [
  { path: "temperature", action: { type: "set" as const, value: 0.2 } },
  { path: "metadata.trace", action: { type: "remove" as const } },
];

const baseForm: ProviderFormState = {
  id: "1",
  name: "anthropic",
  label: "Anthropic",
  channel: "anthropic",
  settings: { rewrite_rules: JSON.stringify(initialRules) },
  routingRules: [],
};

describe("RewriteRulesTab", () => {
  it("persists the fresh rewrite_rules JSON after deleting an existing rule", async () => {
    const container = document.createElement("div");
    const root = createRoot(container);
    const onSave = vi.fn();

    try {
      await act(async () => {
        root.render(
          <I18nProvider>
            <RewriteRulesTab
              form={baseForm}
              onChange={() => {}}
              onSave={onSave}
              notify={() => {}}
            />
          </I18nProvider>,
        );
      });

      await act(async () => {
        getButtonContaining(container, "temperature").click();
      });

      await act(async () => {
        getButtonByText(container, "Delete").click();
      });

      expect(onSave).toHaveBeenCalledWith(JSON.stringify([initialRules[1]]));
    } finally {
      await act(async () => {
        root.unmount();
      });
    }
  });
});
