import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateBanner } from "./update-banner";

let automaticCheckEnabled = false;
const queryCalls: Array<{ queryKey?: string[]; enabled?: boolean }> = [];

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@tanstack/react-query", () => ({
  queryOptions: <T,>(options: T) => options,
  useQuery: (options: { queryKey?: string[]; enabled?: boolean }) => {
    queryCalls.push(options);
    if (options.queryKey?.[0] === "instance-settings") {
      return { data: [{ enable_auto_update_check: automaticCheckEnabled }] };
    }
    if (options.queryKey?.[1] === "status") {
      return { data: { state: "idle" }, isError: false };
    }
    return { data: { available: false } };
  },
}));

describe("UpdateBanner automatic checks", () => {
  beforeEach(() => {
    automaticCheckEnabled = false;
    queryCalls.length = 0;
  });

  it("does not request update status or version checks by default", () => {
    renderToStaticMarkup(<UpdateBanner />);

    expect(queryCalls.find((q) => q.queryKey?.[1] === "status")?.enabled).toBe(false);
    expect(queryCalls.find((q) => q.queryKey?.[1] === "check")?.enabled).toBe(false);
  });

  it("enables both requests after the database preference is enabled", () => {
    automaticCheckEnabled = true;
    renderToStaticMarkup(<UpdateBanner />);

    expect(queryCalls.find((q) => q.queryKey?.[1] === "status")?.enabled).toBe(true);
    expect(queryCalls.find((q) => q.queryKey?.[1] === "check")?.enabled).toBe(true);
  });
});
