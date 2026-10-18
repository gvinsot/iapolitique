import { useId, useState } from "react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { forgetLink, isLinkRemembered, rememberLink } from "@/lib/tracking-storage";

interface RememberLinkToggleProps {
  token: string;
  /** Reminder stored with the link, e.g. "Idée envoyée le 18 octobre 2026". */
  label: string;
}

/**
 * Opt-in: keeps the tracking link in this browser's localStorage, only once the visitor ticks
 * the box (unticked by default, AGENTS.md §7.5). Unticking forgets it.
 */
export function RememberLinkToggle({ token, label }: RememberLinkToggleProps) {
  const id = useId();
  const [remembered, setRemembered] = useState(() => isLinkRemembered(token));
  const [failed, setFailed] = useState(false);

  return (
    <div className="space-y-1">
      <div className="flex items-start gap-3">
        <Checkbox
          id={id}
          checked={remembered}
          onCheckedChange={(checked) => {
            const next = checked === true;
            const ok = next ? rememberLink(token, label) : forgetLink(token);
            setFailed(!ok);
            if (ok) setRemembered(next);
          }}
          aria-describedby={`${id}-help`}
          className="mt-0.5"
        />
        <Label htmlFor={id} className="block leading-snug font-normal">
          Se souvenir de ce lien sur cet appareil
        </Label>
      </div>
      <p id={`${id}-help`} className="pl-7 text-xs text-muted-foreground">
        Le lien est alors enregistré dans ce navigateur uniquement, jamais transmis (voir la{" "}
        <Link
          to={`${paths.privacy}#stockage-local`}
          className="underline underline-offset-4 hover:no-underline"
        >
          politique de confidentialité
        </Link>
        ). Ne cochez pas sur un appareil partagé.
      </p>
      {failed ? (
        <p role="alert" className="pl-7 text-sm text-destructive">
          Ce navigateur refuse l’enregistrement local (navigation privée ou stockage bloqué) :
          conservez le lien autrement.
        </p>
      ) : null}
    </div>
  );
}
