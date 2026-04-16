// @vitest-environment jsdom

import { act } from "react";
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";

import { BetaHeadersEditor } from "./BetaHeadersEditor";

const TEST_BETA = "prompt-caching-2024-07-31";

// @ts-expect-error React's test-only act flag is intentionally set on globalThis.
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

function translate(key: string) {
  return key;
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

function Harness() {
  const [value, setValue] = useState("[]");
  return <BetaHeadersEditor value={value} onChange={setValue} t={translate} />;
}

describe("BetaHeadersEditor", () => {
  it("reflects the selected beta with aria-pressed after a click", async () => {
    const container = document.createElement("div");
    const root = createRoot(container);

    try {
      await act(async () => {
        root.render(React.createElement(Harness));
      });

      const expandButton = getButtonByText(container, "common.show");
      await act(async () => {
        expandButton.click();
      });

      const betaButton = getButtonByText(container, TEST_BETA);
      expect(betaButton.getAttribute("aria-pressed")).toBe("false");

      await act(async () => {
        betaButton.click();
      });

      expect(getButtonByText(container, TEST_BETA).getAttribute("aria-pressed")).toBe("true");
    } finally {
      await act(async () => {
        root.unmount();
      });
    }
  });
});
