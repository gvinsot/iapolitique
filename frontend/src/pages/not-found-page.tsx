import { CompassIcon } from "lucide-react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { Container } from "@/components/layout/container";
import { Button } from "@/components/ui/button";
import { useDocumentTitle } from "@/hooks/use-document-title";

export function NotFoundContent() {
  return (
    <Container className="flex flex-col items-center gap-6 py-20 text-center">
      <CompassIcon className="size-12 text-muted-foreground" aria-hidden="true" />
      <div className="space-y-3">
        <p className="text-sm font-semibold tracking-wide text-primary uppercase">Erreur 404</p>
        <h1 className="text-3xl font-semibold sm:text-4xl">Page introuvable</h1>
        <p className="max-w-md text-pretty text-muted-foreground">
          L’adresse demandée n’existe pas ou plus. Si vous avez suivi un lien de suivi, vérifiez
          qu’il a été copié en entier.
        </p>
      </div>
      <div className="flex flex-wrap justify-center gap-3">
        <Button asChild>
          <Link to={paths.home}>Retour au classement</Link>
        </Button>
        <Button asChild variant="outline">
          <Link to={paths.propose}>Proposer une idée</Link>
        </Button>
      </div>
    </Container>
  );
}

export function NotFoundPage() {
  useDocumentTitle("Page introuvable");
  return (
    <>
      <meta name="robots" content="noindex" />
      <NotFoundContent />
    </>
  );
}
