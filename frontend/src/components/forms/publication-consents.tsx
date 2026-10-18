import { useId } from "react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import type { PublicationConsents } from "@/lib/consents";
import { RULES_LICENCE_ANCHOR } from "@/lib/rules";

interface PublicationConsentsFieldProps {
  value: PublicationConsents;
  onChange: (value: PublicationConsents) => void;
  error?: string | null;
}

/** Licence and publication consents, two separate boxes, never pre-checked. */
export function PublicationConsentsField({
  value,
  onChange,
  error,
}: PublicationConsentsFieldProps) {
  const id = useId();
  const errorId = `${id}-error`;
  const describedBy = error ? errorId : undefined;
  return (
    <fieldset className="space-y-3">
      <legend className="mb-3 text-sm font-medium">Avant d’envoyer</legend>
      <div className="flex items-start gap-3">
        <Checkbox
          id={`${id}-licence`}
          checked={value.licence}
          onCheckedChange={(checked) => {
            onChange({ ...value, licence: checked === true });
          }}
          aria-invalid={error && !value.licence ? true : undefined}
          aria-describedby={describedBy}
          className="mt-0.5"
        />
        <Label htmlFor={`${id}-licence`} className="block leading-snug font-normal">
          J’accepte la{" "}
          <Link
            to={`${paths.terms}#${RULES_LICENCE_ANCHOR}`}
            className="font-medium text-primary underline underline-offset-4 hover:no-underline"
          >
            licence de ma contribution (article 11 des règles)
          </Link>{" "}
          : je déclare être l’auteur du texte et autorise sa publication anonyme, sa reformulation
          par IA et sa diffusion en données ouvertes.
        </Label>
      </div>
      <div className="flex items-start gap-3">
        <Checkbox
          id={`${id}-publication`}
          checked={value.publication}
          onCheckedChange={(checked) => {
            onChange({ ...value, publication: checked === true });
          }}
          aria-invalid={error && !value.publication ? true : undefined}
          aria-describedby={describedBy}
          className="mt-0.5"
        />
        <Label htmlFor={`${id}-publication`} className="block leading-snug font-normal">
          Je comprends que mon texte sera publié s’il est accepté.
        </Label>
      </div>
      {error ? (
        <p id={errorId} role="alert" className="text-sm font-medium text-destructive">
          {error}
        </p>
      ) : null}
    </fieldset>
  );
}
