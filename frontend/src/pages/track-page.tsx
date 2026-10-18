import { CircleDashedIcon, KeyRoundIcon, LinkIcon } from "lucide-react";
import { Link, useParams } from "react-router";

import { paths } from "@/app/paths";
import { PageHeader } from "@/components/common/page-header";
import { Container } from "@/components/layout/container";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { isWellFormedTrackingToken } from "@/lib/tracking-token";

const stages = [
  { title: "Proposition reçue", description: "Votre texte a été enregistré." },
  {
    title: "Filtrage",
    description: "Vérification du respect, du sujet, de la légalité et du réalisme.",
  },
  {
    title: "Analyse et chiffrage",
    description: "Reformulation neutre, pour et contre, estimation du gain avec sources.",
  },
  { title: "Publication", description: "L’idée rejoint le classement public." },
] as const;

/** Anonymous follow-up of a submission, reached through the private link given once. */
export function TrackPage() {
  const { token } = useParams();
  useDocumentTitle("Suivi de votre proposition");
  const valid = isWellFormedTrackingToken(token);

  return (
    <>
      {/* Private link: keep it out of search engines. */}
      <meta name="robots" content="noindex, nofollow" />
      <PageHeader
        eyebrow="Lien privé"
        title="Suivi de votre proposition"
        description="Cette page indique où en est votre idée, votre contribution ou votre signalement."
      />
      <Container className="grid gap-8 py-10 lg:grid-cols-[2fr_1fr]">
        <div className="space-y-6">
          {valid ? (
            <Card>
              <CardHeader>
                <CardTitle>
                  <h2>Étapes</h2>
                </CardTitle>
                <CardDescription>
                  Le statut détaillé s’affichera ici dès l’ouverture du dépôt.
                </CardDescription>
              </CardHeader>
              <CardContent>
                <ol className="space-y-4">
                  {stages.map((stage) => (
                    <li key={stage.title} className="flex gap-3">
                      <CircleDashedIcon
                        className="mt-0.5 size-5 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                      />
                      <div>
                        <p className="font-medium">
                          {stage.title}
                          <span className="sr-only"> : statut non disponible</span>
                        </p>
                        <p className="text-sm text-muted-foreground">{stage.description}</p>
                      </div>
                    </li>
                  ))}
                </ol>
              </CardContent>
            </Card>
          ) : (
            <Alert variant="destructive">
              <LinkIcon aria-hidden="true" />
              <AlertTitle>Ce lien de suivi n’est pas valide</AlertTitle>
              <AlertDescription>
                <p>
                  Vérifiez qu’il a été copié en entier : un lien de suivi se termine par une suite
                  de 43 caractères (lettres, chiffres, « - » et « _ »).
                </p>
              </AlertDescription>
            </Alert>
          )}
          <Button asChild variant="outline">
            <Link to={paths.home}>Voir le classement des idées</Link>
          </Button>
        </div>

        <aside aria-label="Confidentialité du lien">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <KeyRoundIcon className="size-4" aria-hidden="true" />
                <h2>Un lien personnel et anonyme</h2>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-sm text-muted-foreground">
              <p>
                Ce lien n’est enregistré nulle part en clair : seule une empreinte chiffrée est
                conservée. Il ne permet pas de vous identifier.
              </p>
              <p>
                Toute personne qui le possède peut consulter ce suivi : ne le partagez pas si vous
                souhaitez rester seul à le connaître. En cas de perte, il ne peut pas être retrouvé.
              </p>
            </CardContent>
          </Card>
        </aside>
      </Container>
    </>
  );
}
