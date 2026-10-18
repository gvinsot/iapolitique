import { RotateCwIcon, TriangleAlertIcon } from "lucide-react";
import { useEffect } from "react";
import { isRouteErrorResponse, Link, useRouteError } from "react-router";

import { paths } from "@/app/paths";
import { Container } from "@/components/layout/container";
import { Button } from "@/components/ui/button";
import { useDocumentTitle } from "@/hooks/use-document-title";

import { NotFoundPage } from "./not-found-page";

/**
 * Error boundary of the routes: unknown URLs render the 404 page, any other failure (crash,
 * failed lazy chunk after a deployment) a generic message with a reload button.
 */
export function RouteErrorPage() {
  const error = useRouteError();
  const notFound = isRouteErrorResponse(error) && error.status === 404;

  useEffect(() => {
    if (!notFound) console.error(error);
  }, [error, notFound]);

  if (notFound) return <NotFoundPage />;
  return <UnexpectedError />;
}

function UnexpectedError() {
  useDocumentTitle("Erreur");
  return (
    <Container className="flex flex-col items-center gap-6 py-20 text-center">
      <TriangleAlertIcon className="size-12 text-destructive" aria-hidden="true" />
      <div className="space-y-3">
        <h1 className="text-3xl font-semibold sm:text-4xl">Une erreur inattendue est survenue</h1>
        <p className="max-w-md text-pretty text-muted-foreground">
          La page n’a pas pu s’afficher. Le site a peut-être été mis à jour entre-temps : rechargez
          la page pour réessayer.
        </p>
      </div>
      <div className="flex flex-wrap justify-center gap-3">
        <Button
          type="button"
          onClick={() => {
            window.location.reload();
          }}
        >
          <RotateCwIcon aria-hidden="true" />
          Recharger la page
        </Button>
        <Button asChild variant="outline">
          <Link to={paths.home} reloadDocument>
            Retour à l’accueil
          </Link>
        </Button>
      </div>
    </Container>
  );
}
