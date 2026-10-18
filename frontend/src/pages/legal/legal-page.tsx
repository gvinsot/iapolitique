import { FilePenLineIcon } from "lucide-react";
import type { ReactNode } from "react";

import { PageHeader } from "@/components/common/page-header";
import { Container } from "@/components/layout/container";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { useDocumentTitle } from "@/hooks/use-document-title";

interface LegalPageProps {
  title: string;
  description: string;
  children: ReactNode;
}

/**
 * Shell for legal pages. Their legal content is an open decision (AGENTS.md §11): every page
 * shows a visible "à compléter" banner until a human validates the text.
 */
export function LegalPage({ title, description, children }: LegalPageProps) {
  useDocumentTitle(title);

  return (
    <>
      <PageHeader eyebrow="Informations légales" title={title} description={description} />
      <Container className="space-y-8 py-10">
        <Alert variant="warning" className="max-w-prose">
          <FilePenLineIcon aria-hidden="true" />
          <AlertTitle>Contenu juridique à compléter</AlertTitle>
          <AlertDescription>
            <p>
              Cette page est un modèle provisoire. Les passages marqués « à compléter » doivent être
              rédigés et validés avant l’ouverture publique du site.
            </p>
          </AlertDescription>
        </Alert>
        <div className="prose-content">{children}</div>
      </Container>
    </>
  );
}

/** Visibly marks a passage that still has to be written. */
export function ToComplete({ children }: { children?: ReactNode }) {
  return (
    <p className="rounded-md border border-dashed border-warning/60 bg-warning-surface px-3 py-2 text-sm text-warning">
      <strong>À compléter :</strong> {children}
    </p>
  );
}
