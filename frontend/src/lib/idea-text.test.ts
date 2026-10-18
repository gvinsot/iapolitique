import { describe, expect, it } from "vitest";

import { formatCharacterCount, ideaTextLength, validateIdeaText } from "./idea-text";

describe("idea text validation", () => {
  it("counts trimmed Unicode characters", () => {
    expect(ideaTextLength("  été  ")).toBe(3);
    expect(ideaTextLength("💶💶")).toBe(2);
  });

  it("enforces the 50 to 4000 characters bounds", () => {
    expect(validateIdeaText("   ")).toMatch(/Décrivez votre idée/);
    expect(validateIdeaText("a".repeat(49))).toMatch(/au moins 50 caractères \(actuellement 49\)/);
    expect(validateIdeaText("a".repeat(50))).toBeNull();
    expect(validateIdeaText("a".repeat(4000))).toBeNull();
    expect(validateIdeaText("a".repeat(4001))).toMatch(/dépasser 4\s000 caractères/);
  });

  it("formats the counter", () => {
    expect(formatCharacterCount(1234).replace(/\s/g, " ")).toBe("1 234 / 4 000 caractères");
  });
});
