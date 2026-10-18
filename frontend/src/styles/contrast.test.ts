import { describe, expect, it } from "vitest";

import css from "./tokens.css?raw";

/**
 * Guards the accessibility of the design tokens: every pair below must keep a WCAG 2.2 AA
 * contrast ratio in both themes. Change a token, this test tells you if it is still readable.
 */

type Theme = "light" | "dark";

function parseBlock(selector: string): Map<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start === -1) throw new Error(`Selector ${selector} not found in tokens.css`);
  const end = css.indexOf("}", start);
  const body = css.slice(start, end);
  const tokens = new Map<string, string>();
  for (const match of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) {
    const [, name, value] = match;
    if (name && value) tokens.set(name, value.trim());
  }
  return tokens;
}

const themes: Record<Theme, Map<string, string>> = {
  light: parseBlock(":root"),
  dark: new Map([...parseBlock(":root"), ...parseBlock(":root.dark")]),
};

/** OKLCH -> relative luminance (WCAG), through linear sRGB, clamped to the sRGB gamut. */
function luminance(value: string): number {
  const match = /^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\)$/.exec(value);
  if (!match) throw new Error(`Unsupported colour value: ${value}`);
  const [l, c, h] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const hue = (h * Math.PI) / 180;
  const a = c * Math.cos(hue);
  const b = c * Math.sin(hue);

  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;

  const clamp = (x: number) => Math.min(1, Math.max(0, x));
  const red = clamp(4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_);
  const green = clamp(-1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_);
  const blue = clamp(-0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_);
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrast(theme: Theme, fg: string, bg: string): number {
  const tokens = themes[theme];
  const fgValue = tokens.get(fg);
  const bgValue = tokens.get(bg);
  if (!fgValue || !bgValue) throw new Error(`Missing token ${fg} or ${bg} in ${theme} theme`);
  const [hi, lo] = [luminance(fgValue), luminance(bgValue)].sort((x, y) => y - x) as [
    number,
    number,
  ];
  return (hi + 0.05) / (lo + 0.05);
}

const TEXT = 4.5;
const UI = 3;

const pairs: [fg: string, bg: string, minimum: number][] = [
  ["foreground", "background", TEXT],
  ["card-foreground", "card", TEXT],
  ["popover-foreground", "popover", TEXT],
  ["primary-foreground", "primary", TEXT],
  ["primary", "background", TEXT],
  ["primary", "card", TEXT],
  ["secondary-foreground", "secondary", TEXT],
  ["muted-foreground", "background", TEXT],
  ["muted-foreground", "muted", TEXT],
  ["muted-foreground", "card", TEXT],
  ["accent-foreground", "accent", TEXT],
  ["destructive-foreground", "destructive", TEXT],
  ["destructive", "background", TEXT],
  ["destructive", "card", TEXT],
  ["warning", "warning-surface", TEXT],
  ["warning", "background", TEXT],
  ["input", "background", UI],
  ["input", "card", UI],
  ["ring", "background", UI],
  ["ring", "card", UI],
];

describe.each<Theme>(["light", "dark"])("%s theme contrast", (theme) => {
  it.each(pairs)("%s on %s reaches %d:1", (fg, bg, minimum) => {
    expect(contrast(theme, fg, bg)).toBeGreaterThanOrEqual(minimum);
  });
});
