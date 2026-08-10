import { describe, expect, it } from "vitest";
import { selectNotesSection, sortReleaseNotesDescending } from "./release-notes";

const bilingual = `## v2.2.5

### English

#### Fixed
- English note

### 简体中文

#### 修复
- 中文说明`;

describe("selectNotesSection", () => {
  it("selects English notes", () => {
    expect(selectNotesSection(bilingual, "en")).toBe("#### Fixed\n- English note");
  });

  it("selects Chinese notes for any Chinese locale", () => {
    expect(selectNotesSection(bilingual, "zh-TW")).toBe("#### 修复\n- 中文说明");
  });

  it("returns an unsectioned body unchanged apart from surrounding whitespace", () => {
    expect(selectNotesSection("  Rolling staging build.\n", "en")).toBe("Rolling staging build.");
  });
});

describe("sortReleaseNotesDescending", () => {
  it("sorts stable versions numerically without mutating the response", () => {
    const entries = [
      { version: "2.9.0", body: "older" },
      { version: "2.10.0", body: "newer" },
      { version: "2.9.1", body: "middle" },
    ];

    expect(sortReleaseNotesDescending(entries).map(({ version }) => version)).toEqual([
      "2.10.0",
      "2.9.1",
      "2.9.0",
    ]);
    expect(entries[0].version).toBe("2.9.0");
  });
});
