import { describe, expect, it } from "vitest";

import { taxonomyFixture } from "@/test/fixtures/reference";

import { adminLevelLabels } from "./labels";
import { labelledOptions, labelOf, taxonomyLabels } from "./taxonomy-labels";

describe("taxonomyLabels", () => {
  it("falls back to the typed labels without a taxonomy", () => {
    const labels = taxonomyLabels();
    expect(labels.fromApi).toBe(false);
    expect(labelOf(labels.adminLevels, "commune")).toBe("Commune");
    expect(labels.categories).toEqual([]);
  });

  it("prefers the labels served by the API and keeps the fallback for the others", () => {
    const labels = taxonomyLabels(taxonomyFixture);
    expect(labels.fromApi).toBe(true);
    expect(labelOf(labels.adminLevels, "commune")).toBe("Commune (libellé de la taxonomie)");
    expect(labelOf(labels.adminLevels, "region")).toBe("Région");
    expect(labelOf(labels.contributionKinds, "doublon")).toBe("Proposition similaire");
    expect(labels.categories.map((category) => category.key)).toEqual([
      "achats_publics",
      "transports",
    ]);
  });

  it("shows an unknown key as is", () => {
    expect(labelOf(taxonomyLabels().ideaKinds, "nouveau_type")).toBe("nouveau_type");
    expect(labelOf(taxonomyLabels().ideaKinds, "toString")).toBe("toString");
  });

  it("lists the options of an enumeration in a stable order", () => {
    const options = labelledOptions(adminLevelLabels, taxonomyLabels(taxonomyFixture).adminLevels);
    expect(options[0]).toEqual(["etat", "État"]);
    expect(options.find(([key]) => key === "commune")?.[1]).toBe(
      "Commune (libellé de la taxonomie)",
    );
    expect(options).toHaveLength(Object.keys(adminLevelLabels).length);
  });
});
