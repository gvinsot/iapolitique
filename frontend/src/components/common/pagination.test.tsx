import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { paginationItems } from "@/lib/pagination";
import { renderComponent } from "@/test/render-component";

import { Pagination } from "./pagination";

describe("paginationItems", () => {
  it("keeps first, last, current and neighbours with gaps", () => {
    expect(paginationItems(1, 1)).toEqual([1]);
    expect(paginationItems(1, 3)).toEqual([1, 2, 3]);
    expect(paginationItems(5, 10)).toEqual([1, null, 4, 5, 6, null, 10]);
    expect(paginationItems(2, 10)).toEqual([1, 2, 3, null, 10]);
  });
});

describe("Pagination", () => {
  const hrefForPage = (page: number) => `?sort=gain&page=${page}`;

  it("renders nothing for a single page", () => {
    renderComponent(<Pagination page={1} pageCount={1} hrefForPage={hrefForPage} />);
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });

  it("links every page and marks the current one", () => {
    renderComponent(<Pagination page={5} pageCount={10} hrefForPage={hrefForPage} />);
    const nav = screen.getByRole("navigation", { name: "Pagination" });
    expect(within(nav).getByRole("link", { name: "Page 5" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: "Page 10" })).toHaveAttribute(
      "href",
      "/?sort=gain&page=10",
    );
    expect(within(nav).getByRole("link", { name: /Précédente/ })).toHaveAttribute(
      "href",
      "/?sort=gain&page=4",
    );
    expect(within(nav).getByRole("link", { name: /Suivante/ })).toHaveAttribute(
      "href",
      "/?sort=gain&page=6",
    );
  });

  it("has no previous link on the first page", () => {
    renderComponent(<Pagination page={1} pageCount={3} hrefForPage={hrefForPage} />);
    expect(screen.queryByRole("link", { name: /Précédente/ })).toBeNull();
    expect(screen.getByRole("link", { name: /Suivante/ })).toBeInTheDocument();
  });

  it("has no next link on the last page and clamps an out-of-range page", () => {
    renderComponent(<Pagination page={42} pageCount={3} hrefForPage={hrefForPage} />);
    expect(screen.queryByRole("link", { name: /Suivante/ })).toBeNull();
    expect(screen.getByRole("link", { name: "Page 3" })).toHaveAttribute("aria-current", "page");
  });
});
