import { BotIcon } from "lucide-react";
import { type ComponentProps, type ReactNode, useId } from "react";

import { aiJsonLdScript, type AiJsonLd } from "@/lib/ai-marking";
import { cn } from "@/lib/utils";

/** Exact wording required before every generated block (AGENTS.md §7.5, AI Act art. 50). */
export const AI_NOTICE_LEAD =
  "Analyse générée automatiquement par intelligence artificielle (Mistral AI), sans relecture humaine.";
export const AI_NOTICE_DETAIL =
  "Elle peut contenir des erreurs ; vérifiez les sources citées. Ce n’est pas une position officielle.";

/** Visible, accessible label announcing AI-generated content. */
export function AiGeneratedNotice({ className }: { className?: string }) {
  const id = useId();
  return (
    <div
      role="note"
      aria-labelledby={id}
      className={cn(
        "flex items-start gap-3 rounded-lg border border-primary/30 bg-accent px-4 py-3 text-sm text-accent-foreground",
        className,
      )}
    >
      <BotIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <p id={id}>
        <strong className="font-semibold">{AI_NOTICE_LEAD}</strong> {AI_NOTICE_DETAIL}
      </p>
    </div>
  );
}

interface AiGeneratedProps extends ComponentProps<"section"> {
  children: ReactNode;
  /** Machine-readable provenance, rendered as JSON-LD next to the content. */
  jsonLd?: AiJsonLd;
  /** Hides the visible notice when an enclosing block already shows it. */
  withNotice?: boolean;
}

/**
 * Wraps a generated block: machine-readable marking (`data-ai-generated="true"`, optional
 * JSON-LD) and the visible notice placed before the content.
 */
export function AiGenerated({
  children,
  jsonLd,
  withNotice = true,
  className,
  ...props
}: AiGeneratedProps) {
  return (
    <section data-ai-generated="true" className={cn("space-y-4", className)} {...props}>
      {withNotice ? <AiGeneratedNotice /> : null}
      {jsonLd ? <JsonLd data={jsonLd} /> : null}
      {children}
    </section>
  );
}

/** Renders structured data. The JSON is escaped so it can never close the script element. */
export function JsonLd({ data }: { data: AiJsonLd }) {
  return <script type="application/ld+json">{aiJsonLdScript(data)}</script>;
}
