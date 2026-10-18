import { KeyRoundIcon, TriangleAlertIcon } from "lucide-react";
import { useId } from "react";

import { CopyButton } from "@/components/common/copy-button";
import { trackingUrl } from "@/lib/tracking-token";

import { RememberLinkToggle } from "./remember-link-toggle";

interface TrackingLinkRevealProps {
  token: string;
  /** Reminder stored with the link if the visitor asks to remember it. */
  label: string;
}

/**
 * The tracking link, displayed once right after a submission (AGENTS.md §7.1): the API keeps
 * only its hash, so it can never be shown again or recovered.
 */
export function TrackingLinkReveal({ token, label }: TrackingLinkRevealProps) {
  const headingId = useId();
  const url = trackingUrl(token);
  return (
    <section
      aria-labelledby={headingId}
      className="space-y-4 rounded-lg border-2 border-warning/60 bg-warning-surface p-4 sm:p-5"
    >
      <h2 id={headingId} className="flex items-center gap-2 text-lg font-semibold text-warning">
        <KeyRoundIcon className="size-5 shrink-0" aria-hidden="true" />
        Votre lien de suivi personnel
      </h2>
      <p className="flex items-start gap-2 font-semibold text-warning">
        <TriangleAlertIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
        Copiez ce lien maintenant : il ne sera plus jamais affiché et ne peut pas être retrouvé.
      </p>
      <p className="text-sm">
        Il est le seul moyen de suivre votre envoi, de le supprimer ou de demander un réexamen.
        Toute personne qui le possède peut le faire : gardez-le pour vous.
      </p>
      <p className="rounded-md border bg-card p-3 font-mono text-sm break-all select-all">{url}</p>
      <CopyButton value={url} label="Copier le lien" variant="default" />
      <RememberLinkToggle token={token} label={label} />
    </section>
  );
}
