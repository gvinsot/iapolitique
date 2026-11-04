import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { useDocumentTitle } from "@/hooks/use-document-title";
import { useCanonicalLink, useCanonicalPath, usePageDescription } from "@/hooks/use-page-meta";

function meta(selector: string): string | null {
  return document.head.querySelector<HTMLMetaElement>(selector)?.content ?? null;
}

function canonicals(): string[] {
  return [...document.head.querySelectorAll<HTMLLinkElement>('link[rel="canonical"]')].map(
    (link) => link.href,
  );
}

describe("page metadata hooks", () => {
  afterEach(() => {
    document.head.innerHTML = "";
  });

  it("sets the description and og:description, then restores the defaults", () => {
    document.head.innerHTML =
      '<meta name="description" content="défaut" /><meta property="og:description" content="défaut og" />';
    const { unmount } = renderHook(() => {
      usePageDescription("Description de la page");
    });
    expect(meta('meta[name="description"]')).toBe("Description de la page");
    expect(meta('meta[property="og:description"]')).toBe("Description de la page");
    unmount();
    expect(meta('meta[name="description"]')).toBe("défaut");
    expect(meta('meta[property="og:description"]')).toBe("défaut og");
  });

  it("mirrors the document title in og:title", () => {
    const { unmount } = renderHook(() => {
      useDocumentTitle("Méthodologie");
    });
    expect(document.title).toBe("Méthodologie · iapolitique");
    expect(meta('meta[property="og:title"]')).toBe("Méthodologie · iapolitique");
    unmount();
    expect(meta('meta[property="og:title"]')).toBeNull();
  });

  it("adds one canonical link and og:url, removed with the page", () => {
    const { unmount } = renderHook(() => {
      useCanonicalLink("https://iapolitique.fr/idees/une-idee");
    });
    expect(canonicals()).toEqual(["https://iapolitique.fr/idees/une-idee"]);
    expect(meta('meta[property="og:url"]')).toBe("https://iapolitique.fr/idees/une-idee");
    unmount();
    expect(canonicals()).toEqual([]);
    expect(meta('meta[property="og:url"]')).toBeNull();
  });

  it("ignores a canonical URL that is not http(s)", () => {
    renderHook(() => {
      useCanonicalLink("javascript:alert(1)");
    });
    expect(canonicals()).toEqual([]);
  });

  it("builds the canonical link of a portal page on the current origin, without query", () => {
    window.history.replaceState(null, "", "/?tri=gain#haut");
    renderHook(() => {
      useCanonicalPath("/");
    });
    expect(canonicals()).toEqual([`${window.location.origin}/`]);
  });
});
