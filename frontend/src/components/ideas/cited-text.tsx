import { Fragment } from "react";

import { sourceAnchor, splitCitations } from "@/lib/citations";

interface CitedTextProps {
  text: string;
  /** Ids of the sources listed on the page ("S1"…): other references stay plain text. */
  sourceIds: ReadonlySet<string>;
}

/**
 * Renders a generated text where `[S1]`-style references become links to the matching entry of
 * the sources list. The text itself is always rendered as plain text, never as HTML.
 */
export function CitedText({ text, sourceIds }: CitedTextProps) {
  return (
    <>
      {splitCitations(text).map((part, index) =>
        part.type === "citation" && sourceIds.has(part.id) ? (
          <a
            key={index}
            href={`#${sourceAnchor(part.id)}`}
            className="mx-0.5 rounded-sm align-super text-xs font-semibold text-primary underline-offset-2 hover:underline"
            aria-label={`Source ${part.id.slice(1)}`}
          >
            [{part.id}]
          </a>
        ) : (
          <Fragment key={index}>{part.type === "citation" ? `[${part.id}]` : part.text}</Fragment>
        ),
      )}
    </>
  );
}
