import { describe, expect, it } from "vitest";

import { aiAnalysisJsonLd, aiJsonLdScript } from "./ai-marking";

/**
 * AGENTS.md §14.1: the JSON-LD is the only structured data written into a <script> element. Its
 * serialiser must escape everything that could end the element, open a comment or break a
 * JavaScript parser, while staying valid JSON with the same value.
 */
describe("aiJsonLdScript", () => {
  const hostile = {
    name: "</script><script>alert(1)</script><!-- --> & \u2028ligne\u2029paragraphe </SCRIPT >",
    nested: { list: ["<img src=x onerror=alert(1)>", "a&b", "\u2028"] },
  };

  it("escapes <, >, &, U+2028 and U+2029", () => {
    const script = aiJsonLdScript(hostile);
    expect(script).not.toMatch(/[<>&\u2028\u2029]/);
    expect(script.toLowerCase()).not.toContain("</script");
    expect(script).not.toContain("<!--");
    expect(script).toContain("\\u003c/script\\u003e");
    expect(script).toContain("\\u0026");
    expect(script).toContain("\\u2028");
    expect(script).toContain("\\u2029");
  });

  it("stays valid JSON with the same value", () => {
    expect(JSON.parse(aiJsonLdScript(hostile))).toEqual(hostile);
  });

  it("escapes the marking of a generated analysis", () => {
    const script = aiJsonLdScript(
      aiAnalysisJsonLd({
        title: "Titre </script>\u2028",
        url: "https://iapolitique.fr/idees/exemple",
        model: "mistral-large-latest",
        generatedAt: "2026-10-12T08:30:00Z",
        analysisVersion: "3",
      }),
    );
    expect(script).not.toMatch(/[<>&\u2028\u2029]/);
    expect(JSON.parse(script)).toMatchObject({ name: "Titre </script>\u2028" });
  });
});
