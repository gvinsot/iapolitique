import axe from "axe-core";

/**
 * Automated accessibility audit (axe-core, WCAG 2.x A/AA rules and best practices) of the
 * rendered document. jsdom has no layout engine: colour contrast is covered by src/styles/contrast.test.ts instead.
 */
export async function audit(): Promise<string[]> {
  const results = await axe.run(document.body, {
    runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"] },
    rules: {
      "color-contrast": { enabled: false },
      // Radix portals render outside landmarks by design (toasts, popovers).
      region: { enabled: false },
    },
  });
  return results.violations.map(
    (violation) =>
      `${violation.id}: ${violation.help} (${violation.nodes.map((node) => node.target.join(" ")).join(", ")})`,
  );
}
