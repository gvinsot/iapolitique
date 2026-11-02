import { describe, expect, it } from "vitest";

import { shareTargets, shareText } from "./share";

const page = {
  url: "https://iapolitique.fr/idees/mutualiser-achats",
  title: "Mutualiser & réduire ?",
};

describe("shareTargets", () => {
  it("encodes the canonical URL and title in every link", () => {
    const targets = shareTargets(page);
    expect(targets.map((t) => t.key)).toEqual([
      "facebook",
      "x",
      "linkedin",
      "bluesky",
      "whatsapp",
      "telegram",
      "email",
    ]);
    for (const target of targets) {
      expect(target.href).toContain(encodeURIComponent(page.url));
      expect(target.href).not.toContain("&réduire");
      expect(target.href).not.toContain(" ");
    }
  });

  it("only uses https share addresses, or mailto for the e-mail", () => {
    for (const target of shareTargets(page)) {
      const { protocol } = new URL(target.href);
      expect(target.key === "email" ? "mailto:" : "https:").toBe(protocol);
    }
  });

  it("decodes back to the title and URL", () => {
    const x = new URL(shareTargets(page).find((t) => t.key === "x")?.href ?? "");
    expect(x.searchParams.get("text")).toBe(shareText(page.title));
    expect(x.searchParams.get("url")).toBe(page.url);
  });
});
