import { useMutation } from "@tanstack/react-query";
import { CheckIcon, PlusIcon, TriangleAlertIcon, XIcon } from "lucide-react";

import { errorMessage } from "@/api/errors";
import { submitIdea } from "@/api/submissions";
import { PageHeader } from "@/components/common/page-header";
import { ProposeForm } from "@/components/ideas/propose-form";
import { Container } from "@/components/layout/container";
import { SubmissionOutcome } from "@/components/tracking/submission-outcome";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { formatDate } from "@/lib/format";

const dos = [
  "Viser une dépense ou une recette d’une administration publique française (État, sécurité sociale, collectivités).",
  "Décrire une mesure concrète, que l’on pourrait mettre en œuvre.",
  "Rester factuel et respectueux.",
];

const donts = [
  "Nommer une personne, un élu, un parti ou un groupe (religion, origine…).",
  "Donner des informations personnelles, sur vous ou sur autrui.",
  "Proposer une mesure contraire aux droits fondamentaux.",
];

export function ProposePage() {
  useDocumentTitle("Proposer une idée");
  const submission = useMutation({
    mutationFn: submitIdea,
    // Shown inline next to the form, which keeps its content for a retry.
    meta: { errorToast: false },
  });
  const result = submission.data;

  return (
    <>
      <PageHeader
        eyebrow="Dépôt anonyme"
        title="Proposer une idée"
        description="Une mesure pour réduire une dépense ou augmenter une recette publique ? Décrivez-la : elle sera vérifiée en quelques secondes, puis améliorée et chiffrée par IA si elle est retenue."
      />
      <Container className="grid gap-8 py-10 lg:grid-cols-[2fr_1fr]">
        <section aria-label="Formulaire de dépôt" className="min-w-0 space-y-6">
          {result ? (
            <>
              <SubmissionOutcome
                kind="idea"
                result={result}
                linkLabel={`Idée envoyée le ${formatDate(new Date().toISOString())}`}
              />
              <p className="text-sm text-muted-foreground">
                Une fois cette page quittée, le lien de suivi ne sera plus affiché.
              </p>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  submission.reset();
                }}
              >
                <PlusIcon aria-hidden="true" />
                J’ai conservé mon lien : proposer une autre idée
              </Button>
            </>
          ) : (
            <>
              {submission.isError ? (
                <Alert variant="destructive" role="alert">
                  <TriangleAlertIcon aria-hidden="true" />
                  <AlertTitle>Votre idée n’a pas pu être envoyée</AlertTitle>
                  <AlertDescription>
                    <p>{errorMessage(submission.error)}</p>
                  </AlertDescription>
                </Alert>
              ) : null}
              <ProposeForm
                onSubmit={(values) => submission.mutateAsync(values).then(() => undefined)}
                submitting={submission.isPending}
              />
            </>
          )}
        </section>

        <aside aria-label="Conseils" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>
                <h2>Pour qu’une idée soit retenue</h2>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4 text-sm">
              <ul className="space-y-2">
                {dos.map((item) => (
                  <li key={item} className="flex gap-2">
                    <CheckIcon className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden="true" />
                    <span>{item}</span>
                  </li>
                ))}
              </ul>
              <p className="font-medium">À éviter :</p>
              <ul className="space-y-2">
                {donts.map((item) => (
                  <li key={item} className="flex gap-2">
                    <XIcon
                      className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                      aria-hidden="true"
                    />
                    <span>{item}</span>
                  </li>
                ))}
              </ul>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>
                <h2>Et ensuite ?</h2>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-sm text-muted-foreground">
              <p>
                Vous obtenez immédiatement la réponse du filtre : idée conservée, ou refusée avec le
                motif pour vous permettre de la reformuler.
              </p>
              <p>
                Un <strong className="text-foreground">lien de suivi personnel</strong> s’affiche
                une seule fois : conservez-le pour suivre l’analyse et la publication de votre idée.
                Il ne permet pas de vous identifier.
              </p>
            </CardContent>
          </Card>
        </aside>
      </Container>
    </>
  );
}
