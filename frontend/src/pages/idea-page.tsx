import { ArrowLeftIcon, InfoIcon } from "lucide-react";
import { Link, useParams } from "react-router";

import { paths } from "@/app/paths";
import { Container } from "@/components/layout/container";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useDocumentTitle } from "@/hooks/use-document-title";

const scoreComponents = [
  "Gain net annuel",
  "Faisabilité",
  "Confiance des données",
  "Délai avant effet",
] as const;

const sections = [
  { value: "analyse", label: "Analyse" },
  { value: "chiffrage", label: "Chiffrage" },
  { value: "impacts", label: "Impacts" },
  { value: "sources", label: "Sources" },
  { value: "contributions", label: "Contributions" },
] as const;

function ParagraphSkeleton() {
  return (
    <div className="space-y-2" aria-hidden="true">
      <Skeleton className="h-4 w-full" />
      <Skeleton className="h-4 w-11/12" />
      <Skeleton className="h-4 w-4/5" />
    </div>
  );
}

/** Idea detail. The layout is final; the content is wired to GET /api/ideas/{slug} later. */
export function IdeaPage() {
  const { slug = "" } = useParams();
  useDocumentTitle("Détail d’une idée");

  return (
    <Container className="space-y-8 py-8 sm:py-10">
      <Link
        to={paths.home}
        className="inline-flex items-center gap-2 rounded-sm text-sm font-medium text-muted-foreground hover:text-foreground"
      >
        <ArrowLeftIcon className="size-4" aria-hidden="true" />
        Retour au classement
      </Link>

      <header className="space-y-4">
        <div className="flex flex-wrap gap-2" aria-hidden="true">
          <Skeleton className="h-6 w-24 rounded-full" />
          <Skeleton className="h-6 w-32 rounded-full" />
        </div>
        <h1 className="sr-only">Détail de l’idée « {slug} »</h1>
        <Skeleton className="h-9 w-3/4" aria-hidden="true" />
        <ParagraphSkeleton />
        <Badge variant="secondary">Analyse générée par IA</Badge>
      </header>

      <Alert variant="info">
        <InfoIcon aria-hidden="true" />
        <AlertTitle>Fiche en préparation</AlertTitle>
        <AlertDescription>
          Le détail des idées (reformulation neutre, pour et contre, chiffrage, impacts et sources)
          s’affichera ici dès la publication des premières analyses.
        </AlertDescription>
      </Alert>

      <section aria-labelledby="score-titre" className="space-y-4">
        <h2 id="score-titre" className="text-xl font-semibold">
          Composantes du score
        </h2>
        <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {scoreComponents.map((label) => (
            <li key={label} className="space-y-3 rounded-lg border bg-card p-4">
              <p className="text-sm font-medium text-muted-foreground">
                {label}
                <span className="sr-only"> : non disponible</span>
              </p>
              <Skeleton className="h-7 w-24" aria-hidden="true" />
            </li>
          ))}
        </ul>
      </section>

      <Tabs defaultValue="analyse">
        <TabsList className="w-full justify-start overflow-x-auto">
          {sections.map((section) => (
            <TabsTrigger key={section.value} value={section.value}>
              {section.label}
            </TabsTrigger>
          ))}
        </TabsList>
        {sections.map((section) => (
          <TabsContent key={section.value} value={section.value} className="space-y-6 pt-4">
            <h2 className="sr-only">{section.label}</h2>
            <ParagraphSkeleton />
            <ParagraphSkeleton />
          </TabsContent>
        ))}
      </Tabs>
    </Container>
  );
}
