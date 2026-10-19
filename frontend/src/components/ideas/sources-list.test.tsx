import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { Source } from "@/lib/idea-model";
import { renderComponent } from "@/test/render-component";

import { SourcesList } from "./sources-list";

const source = (id: string, url: string): Source => ({
  id,
  title: `Titre ${id}`,
  publisher: "Éditeur",
  url,
  used_for: "Chiffrage",
});

/** AGENTS.md §14.1: only http(s) source URLs become links; every other scheme stays text. */
describe("SourcesList links", () => {
  it.each([
    ["https://www.insee.fr/fr/statistiques", "https://www.insee.fr/fr/statistiques"],
    ["http://www.senat.fr/rap/r24-001.html", "http://www.senat.fr/rap/r24-001.html"],
    ["HTTPS://Example.ORG/a b", "https://example.org/a%20b"],
  ])("links %s with a hardened rel", (url, href) => {
    renderComponent(<SourcesList sources={[source("S1", url)]} />);
    const link = screen.getByRole("link", { name: /Titre S1/ });
    expect(link).toHaveAttribute("href", href);
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer nofollow ugc");
  });

  it.each([
    "javascript:alert(document.domain)",
    "JavaScript:alert(1)",
    " javascript:alert(1)",
    "java\tscript:alert(1)",
    "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
    "data:image/svg+xml,<svg onload=alert(1)>",
    "vbscript:msgbox(1)",
    "file:///etc/passwd",
    "ftp://example.org/fichier",
    "//evil.example/page",
    "/idees/relative",
    "pas une adresse",
  ])("renders %s as plain text", (url) => {
    const { container } = renderComponent(<SourcesList sources={[source("S1", url)]} />);
    expect(container.querySelector("a")).toBeNull();
    expect(screen.getByText("Titre S1")).toBeInTheDocument();
    expect(container).toHaveTextContent(url, { normalizeWhitespace: false });
    expect(container.querySelector("script, iframe, [onload], [href]")).toBeNull();
  });
});
