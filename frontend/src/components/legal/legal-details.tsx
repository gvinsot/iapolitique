import { RotateCwIcon, TriangleAlertIcon } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { LEGAL_DETAILS_UNAVAILABLE } from "@/lib/legal";

/** One detail coming from the API, or a visible placeholder while it is unavailable. */
export function LegalValue({ value, pending }: { value: string | undefined; pending: boolean }) {
  if (value) return <>{value}</>;
  return (
    <span className="text-muted-foreground italic">
      {pending ? "chargement…" : LEGAL_DETAILS_UNAVAILABLE}
    </span>
  );
}

/** An e-mail address from the API as a mailto link, or the placeholder. */
export function LegalEmail({ email, pending }: { email: string | undefined; pending: boolean }) {
  if (!email) return <LegalValue value={undefined} pending={pending} />;
  return <a href={`mailto:${email}`}>{email}</a>;
}

/** Page-level notice when GET /api/legal failed. */
export function LegalDetailsUnavailable({ onRetry }: { onRetry: () => void }) {
  return (
    <Alert variant="warning" role="status" className="max-w-prose">
      <TriangleAlertIcon aria-hidden="true" />
      <AlertTitle>Coordonnées momentanément indisponibles</AlertTitle>
      <AlertDescription>
        <p>
          Les coordonnées de l’éditeur n’ont pas pu être chargées. Le reste de la page est à jour ;
          réessayez dans quelques instants pour les afficher.
        </p>
        <Button type="button" variant="outline" size="sm" onClick={onRetry} className="mt-2">
          <RotateCwIcon aria-hidden="true" />
          Réessayer
        </Button>
      </AlertDescription>
    </Alert>
  );
}
