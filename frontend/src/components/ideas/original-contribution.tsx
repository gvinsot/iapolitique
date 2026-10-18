import { QuoteIcon } from "lucide-react";
import { useId } from "react";

import { formatDate } from "@/lib/format";
import { CONTRIBUTOR_ATTRIBUTION } from "@/lib/legal";
import { cn } from "@/lib/utils";

export const ORIGINAL_CONTRIBUTION_HEADING =
  "Contribution d’un internaute (texte original, données personnelles masquées)";

/** Anchor of the original text, referenced by the JSON-LD `isBasedOn` (lib/ai-marking.ts). */
export const ORIGINAL_TEXT_ANCHOR = "texte-original";

interface OriginalContributionProps {
  /** Original text as published: the masked version (AGENTS.md §7.5). */
  text: string;
  /** Submission day, `YYYY-MM-DD`. */
  submittedOn?: string | null;
  headingLevel?: 2 | 3;
  className?: string;
}

/**
 * The visitor's own text, visually separated from the AI-generated analysis and never marked as
 * generated. Line breaks of the original are kept; the text is rendered as plain text.
 */
export function OriginalContribution({
  text,
  submittedOn,
  headingLevel = 2,
  className,
}: OriginalContributionProps) {
  const headingId = useId();
  const Heading = headingLevel === 2 ? "h2" : "h3";
  return (
    <section
      id={ORIGINAL_TEXT_ANCHOR}
      aria-labelledby={headingId}
      data-source-type="user"
      className={cn("space-y-3 rounded-lg border border-dashed bg-muted/50 p-4 sm:p-5", className)}
    >
      <Heading id={headingId} className="flex items-start gap-2 text-base font-semibold">
        <QuoteIcon className="mt-1 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        {ORIGINAL_CONTRIBUTION_HEADING}
      </Heading>
      <blockquote className="border-l-2 pl-4 text-pretty whitespace-pre-line">{text}</blockquote>
      <p className="text-sm text-muted-foreground">
        {CONTRIBUTOR_ATTRIBUTION}
        {submittedOn ? <>, proposée le {formatDate(submittedOn)}</> : null}.
      </p>
    </section>
  );
}
