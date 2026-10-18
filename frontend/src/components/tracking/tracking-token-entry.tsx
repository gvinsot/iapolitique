import { ArrowRightIcon, XIcon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";
import { Link, useNavigate } from "react-router";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { formatDate } from "@/lib/format";
import { forgetLink, type RememberedLink, readRememberedLinks } from "@/lib/tracking-storage";
import { parseTrackingInput, TRACKING_PATH, trackingHref } from "@/lib/tracking-token";

/** Opens a tracking link pasted by the visitor; the token goes to the fragment only. */
export function TrackingTokenEntry() {
  const id = useId();
  const navigate = useNavigate();
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const token = parseTrackingInput(value);
    if (!token) {
      setError(
        "Ce lien n’est pas reconnu. Collez le lien complet reçu après votre envoi : il se termine par 43 caractères (lettres, chiffres, « - » et « _ »).",
      );
      return;
    }
    setError(null);
    setValue("");
    void navigate({ pathname: TRACKING_PATH, hash: token }, { replace: true });
  };

  return (
    <form onSubmit={submit} noValidate className="space-y-2" aria-label="Ouvrir un lien de suivi">
      <Label htmlFor={`${id}-link`}>Votre lien de suivi</Label>
      <div className="flex flex-col gap-2 sm:flex-row">
        <Input
          id={`${id}-link`}
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
          }}
          autoComplete="off"
          spellCheck={false}
          placeholder="https://iapolitique.fr/suivi#…"
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? `${id}-error` : undefined}
        />
        <Button type="submit">
          Afficher le suivi
          <ArrowRightIcon aria-hidden="true" />
        </Button>
      </div>
      {error ? (
        <p id={`${id}-error`} role="alert" className="text-sm font-medium text-destructive">
          {error}
        </p>
      ) : null}
    </form>
  );
}

/** Links remembered on this device at the visitor's request, each one can be forgotten. */
export function RememberedLinks() {
  const [links, setLinks] = useState<RememberedLink[]>(readRememberedLinks);
  if (links.length === 0) return null;
  return (
    <section aria-labelledby="liens-memorises" className="space-y-3">
      <h2 id="liens-memorises" className="text-lg font-semibold">
        Liens mémorisés sur cet appareil
      </h2>
      <ul className="space-y-2">
        {links.map((link) => (
          <li
            key={link.token}
            className="flex flex-wrap items-center justify-between gap-2 rounded-lg border bg-card p-3"
          >
            <Link
              to={trackingHref(link.token)}
              className="font-medium text-primary underline underline-offset-4 hover:no-underline"
            >
              {link.label}
            </Link>
            <span className="text-sm text-muted-foreground">
              mémorisé le {formatDate(link.savedOn)}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                forgetLink(link.token);
                setLinks(readRememberedLinks());
              }}
              aria-label={`Oublier le lien « ${link.label} »`}
            >
              <XIcon aria-hidden="true" />
              Oublier
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}
