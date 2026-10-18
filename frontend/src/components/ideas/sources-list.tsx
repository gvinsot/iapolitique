import { ExternalLinkIcon } from "lucide-react";

import { safeExternalUrl, sourceAnchor } from "@/lib/citations";
import { formatDate } from "@/lib/format";
import type { Source } from "@/lib/idea-model";
import { cn } from "@/lib/utils";

/** Sources cited by an analysis; each entry is the target of the `[Sx]` links (CitedText). */
export function SourcesList({ sources, className }: { sources: Source[]; className?: string }) {
  if (sources.length === 0) {
    return <p className="text-sm text-muted-foreground">Aucune source n’est citée.</p>;
  }
  return (
    <ol className={cn("space-y-3", className)}>
      {sources.map((source) => {
        const url = safeExternalUrl(source.url);
        return (
          <li
            key={source.id}
            id={sourceAnchor(source.id)}
            className="flex scroll-mt-24 gap-3 rounded-md p-2 target:bg-accent"
          >
            <span className="font-mono text-sm font-semibold text-muted-foreground">
              [{source.id}]
            </span>
            <div className="min-w-0 space-y-1">
              <p className="font-medium text-pretty">
                {url ? (
                  <a
                    href={url}
                    rel="noreferrer nofollow"
                    className="inline-flex items-baseline gap-1 text-primary underline underline-offset-4 hover:no-underline"
                  >
                    {source.title}
                    <ExternalLinkIcon
                      className="size-3.5 shrink-0 self-center"
                      aria-hidden="true"
                    />
                  </a>
                ) : (
                  source.title
                )}
              </p>
              <p className="text-sm text-muted-foreground">
                {source.publisher}
                {source.date ? <> · {formatDate(source.date)}</> : null}
              </p>
              {source.used_for ? (
                <p className="text-sm">
                  <span className="text-muted-foreground">Utilisée pour : </span>
                  {source.used_for}
                </p>
              ) : null}
              {url ? <p className="text-xs break-all text-muted-foreground">{url}</p> : null}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
