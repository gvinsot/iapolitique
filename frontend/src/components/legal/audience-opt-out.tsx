import { useId, useState } from "react";

import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { isAudienceOptedOut, setAudienceOptOut } from "@/lib/audience";

/** Lets the visitor refuse the audience measurement in this browser (AGENTS.md §7.5). */
export function AudienceOptOut() {
  const id = useId();
  const [optedOut, setOptedOut] = useState(isAudienceOptedOut);
  const [failed, setFailed] = useState(false);

  return (
    <div className="my-4 space-y-1 rounded-lg border p-4">
      <div className="flex items-start gap-3">
        <Checkbox
          id={id}
          checked={optedOut}
          onCheckedChange={(checked) => {
            const next = checked === true;
            const ok = setAudienceOptOut(next);
            setFailed(!ok);
            if (ok) setOptedOut(next);
          }}
          className="mt-0.5"
        />
        <Label htmlFor={id} className="block leading-snug font-normal">
          Refuser la mesure d’audience dans ce navigateur
        </Label>
      </div>
      <p className="pl-7 text-xs text-muted-foreground" aria-live="polite">
        {optedOut
          ? "Vos visites ne sont plus mesurées dans ce navigateur."
          : "Vos visites sont comptées de façon anonyme."}
      </p>
      {failed ? (
        <p role="alert" className="pl-7 text-sm text-destructive">
          Ce navigateur refuse l’enregistrement local (navigation privée ou stockage bloqué) : votre
          choix n’a pas pu être enregistré.
        </p>
      ) : null}
    </div>
  );
}
