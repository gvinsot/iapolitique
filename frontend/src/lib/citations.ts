/** `[S1]`-style source references in generated texts (AGENTS.md §6.2). */

export type CitationPart = { type: "text"; text: string } | { type: "citation"; id: string };

const CITATION_PATTERN = /\[(S\d{1,3})\]/g;

/** Splits a text into plain parts and source references, in order. */
export function splitCitations(text: string): CitationPart[] {
  const parts: CitationPart[] = [];
  let last = 0;
  for (const match of text.matchAll(CITATION_PATTERN)) {
    const [whole, id] = match;
    if (match.index > last) parts.push({ type: "text", text: text.slice(last, match.index) });
    parts.push({ type: "citation", id: id ?? whole });
    last = match.index + whole.length;
  }
  if (last < text.length) parts.push({ type: "text", text: text.slice(last) });
  return parts;
}

/** Anchor of a source entry in the sources list: "source-S1". */
export function sourceAnchor(id: string): string {
  return `source-${id}`;
}

/** Only http(s) links are rendered as links: a generated URL is never trusted blindly. */
export function safeExternalUrl(url: string): string | null {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" || parsed.protocol === "http:" ? parsed.href : null;
  } catch {
    return null;
  }
}
