import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { UpdatePanel } from "./update-panel";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@tanstack/react-query", () => ({
  queryOptions: <T,>(options: T) => options,
  useMutation: () => ({ isPending: false, mutate: vi.fn() }),
  useQuery: (options: { queryKey?: string[] }) => options.queryKey?.[1] === "status"
    ? { data: { state: "unavailable" }, isError: false, isFetching: false }
    : { data: undefined, error: undefined, isError: false, isFetching: false, refetch: vi.fn() },
  useQueryClient: () => ({ invalidateQueries: vi.fn() }),
}));

describe("UpdatePanel", () => {
  it("disables self-update and directs edge deployments to their serverless platform", () => {
    const html = renderToStaticMarkup(<UpdatePanel />);

    expect(html).toContain("unavailable.title");
    expect(html).toContain("unavailable.description");
    expect(html).toContain("disabled");
    expect(html).not.toContain("apply.button");
  });
});
