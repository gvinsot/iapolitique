import { describe, expect, it } from "vitest";

import packageJson from "../../package.json";

/**
 * Source-level guards of AGENTS.md §14.1 and §14.4 over the application code (tests and their
 * helpers excluded), complementing the ESLint rules (which an
 * inline `eslint-disable` could silence): no HTML or Markdown rendering of visitor or AI content,
 * no HTML sink, and browser storage confined to the modules documented in /confidentialite.
 */
const sources = import.meta.glob<string>(
  ["/src/**/*.{ts,tsx}", "!/src/**/*.test.{ts,tsx}", "!/src/test/**"],
  {
    query: "?raw",
    import: "default",
    eager: true,
  },
);
const files = Object.entries(sources).filter(([path]) => !path.endsWith("/api/schema.d.ts"));

function filesMatching(pattern: RegExp): string[] {
  return files.filter(([, text]) => pattern.test(text)).map(([path]) => path);
}

describe("XSS guards", () => {
  it("scans the application sources", () => {
    expect(files.length).toBeGreaterThan(50);
  });

  it("uses no HTML sink nor dynamic code", () => {
    expect(
      filesMatching(
        /dangerouslySetInnerHTML|\.innerHTML|\.outerHTML|insertAdjacentHTML|createContextualFragment|DOMParser|document\.write|\beval\(|new Function\(|srcdoc/,
      ),
    ).toEqual([]);
  });

  it("never silences the XSS lint rules", () => {
    expect(
      filesMatching(
        /eslint-disable[^\n]*(react\/no-danger|jsx-no-script-url|no-restricted-properties)/,
      ),
    ).toEqual([]);
  });

  it("depends on no Markdown or HTML rendering library", () => {
    const dependencies = Object.keys({
      ...packageJson.dependencies,
      ...packageJson.devDependencies,
    });
    const renderers =
      /^(marked|markdown-it|react-markdown|remark.*|rehype.*|micromark|showdown|commonmark|html-react-parser|react-html-parser|dompurify|isomorphic-dompurify|sanitize-html|mdx?|@mdx-js\/.*)$/;
    expect(dependencies.filter((name) => renderers.test(name))).toEqual([]);
  });

  it("keeps localStorage to the theme, the opt-in tracking links and the audience opt-out", () => {
    expect(filesMatching(/localStorage/).sort()).toEqual([
      "/src/components/tracking/remember-link-toggle.tsx",
      "/src/lib/audience.ts",
      "/src/lib/theme.ts",
      "/src/lib/tracking-storage.ts",
      "/src/lib/tracking-token.ts",
    ]);
  });

  it("keeps sessionStorage to the admin token", () => {
    expect(filesMatching(/sessionStorage/).sort()).toEqual([
      "/src/api/admin-token.ts",
      "/src/hooks/use-admin-session.ts",
    ]);
  });
});
