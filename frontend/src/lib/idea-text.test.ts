import { describe, expect, it } from "vitest";

import {
  formatCharacterCount,
  ideaTextLength,
  validateIdeaText,
  validateOptionalEmail,
} from "./idea-text";

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

describe("reporter e-mail validation", () => {
  it("accepts an empty field or a plain address", () => {
    expect(validateOptionalEmail("  ")).toBeNull();
    expect(validateOptionalEmail(" dominique@example.org ")).toBeNull();
  });

  it("refuses an address carrying mailto header fields (INJ-7)", () => {
    expect(
      validateOptionalEmail("contact@mairie.fr?cc=attacker%40evil.example&subject=Re%3A"),
    ).toMatch(/ne semble pas valide/);
    expect(validateOptionalEmail("a@b.c?body=Cliquez")).toMatch(/ne semble pas valide/);
  });
});
