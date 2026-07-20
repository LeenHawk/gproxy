import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { getPaginationItems, Pagination } from "./pagination";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string, values?: { page?: number }) => values?.page ? `${key} ${values.page}` : key }),
}));

describe("getPaginationItems", () => {
  it("shows four leading pages at the start", () => {
    expect(getPaginationItems(1, 102)).toEqual([1, 2, 3, 4, "ellipsis", 101, 102]);
  });

  it("uses two ellipses in the middle and a compact trailing range", () => {
    expect(getPaginationItems(51, 102)).toEqual([1, 2, "ellipsis", 50, 51, 52, "ellipsis", 101, 102]);
    expect(getPaginationItems(100, 102)).toEqual([1, 2, "ellipsis", 99, 100, 101, 102]);
  });
});

describe("Pagination", () => {
  it("renders accessible controlled pagination in SSR", () => {
    const html = renderToStaticMarkup(
      <Pagination page={2} totalPages={3} onPageChange={() => undefined} disabled />,
    );
    expect(html).toContain('aria-label="pagination.label"');
    expect(html).toContain('aria-current="page"');
    expect(html).toContain('aria-label="pagination.page 2"');
    expect(html).toContain("disabled");
  });

  it("does not render a single page", () => {
    expect(renderToStaticMarkup(
      <Pagination page={1} totalPages={1} onPageChange={() => undefined} />,
    )).toBe("");
  });
});
