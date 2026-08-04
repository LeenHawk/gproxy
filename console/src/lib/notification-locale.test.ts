import { describe, expect, it } from "vitest";
import { pickNotificationContent } from "./notification-locale";

describe("notification locale selection", () => {
  it("uses exact, Chinese fallback, then English content", () => {
    const content = {
      en: { title: "English", body: "Body" },
      "zh-CN": { title: "简体", body: "正文" },
      "zh-TW": { title: "繁體", body: "正文" },
    };
    expect(pickNotificationContent(content, "zh-TW").title).toBe("繁體");
    expect(pickNotificationContent({ en: content.en, "zh-CN": content["zh-CN"] }, "zh-TW").title).toBe("简体");
    expect(pickNotificationContent(content, "zh-HK").title).toBe("简体");
    expect(pickNotificationContent(content, "fr").title).toBe("English");
  });
});
