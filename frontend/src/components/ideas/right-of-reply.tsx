import { MessageSquareReplyIcon } from "lucide-react";
import { useId } from "react";

import { formatDate } from "@/lib/format";
import type { RightOfReplyData } from "@/lib/idea-model";

/**
 * Reply published under an idea by the administrator on behalf of a person named or designated
 * (LCEN art. 1-1 III). Shown verbatim, neither generated nor rewritten by AI.
 */
export function RightOfReply({ reply }: { reply: RightOfReplyData }) {
  const headingId = useId();
  return (
    <section
      aria-labelledby={headingId}
      data-source-type="right-of-reply"
      className="space-y-3 rounded-lg border-2 bg-card p-4 sm:p-5"
    >
      <h2 id={headingId} className="flex items-center gap-2 text-lg font-semibold">
        <MessageSquareReplyIcon className="size-5 shrink-0" aria-hidden="true" />
        Droit de réponse
      </h2>
      <p className="text-sm text-muted-foreground">
        Réponse de {reply.author_label}, publiée le {formatDate(reply.published_at)} en application
        de l’article 1-1, III de la loi du 21 juin 2004. Texte reproduit sans modification.
      </p>
      <blockquote className="border-l-2 pl-4 text-pretty whitespace-pre-line">
        {reply.body}
      </blockquote>
    </section>
  );
}
