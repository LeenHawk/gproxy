import { describe, expect, it } from "vitest";

import { getCredentialUsageActionLabels } from "./credential-usage";

describe("credential usage labels", () => {
  const labels = {
    show: "查看限额",
    hide: "收起限额",
    refresh: "刷新",
    loading: "加载中...",
  };

  it("shows the collapsed action label before the quota panel is opened", () => {
    expect(getCredentialUsageActionLabels({ expanded: false, loading: false, labels })).toEqual({
      primary: "查看限额",
      refresh: "刷新",
    });
  });

  it("shows the expanded action label after the quota panel is opened", () => {
    expect(getCredentialUsageActionLabels({ expanded: true, loading: false, labels })).toEqual({
      primary: "收起限额",
      refresh: "刷新",
    });
  });

  it("uses a loading label instead of switching to a different action name while fetching", () => {
    expect(getCredentialUsageActionLabels({ expanded: true, loading: true, labels })).toEqual({
      primary: "加载中...",
      refresh: "加载中...",
    });
  });
});
