import { ArrowRightIcon, LightbulbIcon } from "lucide-react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { ProcessSteps } from "@/components/common/process-steps";
import { IdeaFiltersBar } from "@/components/ideas/idea-filters-bar";
import { IdeaRanking } from "@/components/ideas/idea-ranking";
import { PublicStats } from "@/components/ideas/public-stats";
import { Container } from "@/components/layout/container";
import { Button } from "@/components/ui/button";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { useIdeaFilters } from "@/hooks/use-idea-filters";
import { usePageDescription } from "@/hooks/use-page-meta";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";

export function HomePage() {
  useDocumentTitle();
  usePageDescription(
    "Propositions citoyennes et anonymes pour réduire les dépenses ou augmenter les recettes publiques, analysées et chiffrées par IA avec leurs sources, classées par rentabilité.",
  );
  const { filters } = useIdeaFilters();
  const { categories } = useTaxonomyLabels();

  return (
    <>
      <section aria-labelledby="accueil-titre" className="border-b bg-muted/30">
        <Container className="grid items-center gap-10 py-12 sm:py-16 lg:grid-cols-[1.3fr_1fr]">
          <div className="space-y-6">
            <p className="text-sm font-semibold tracking-wide text-primary uppercase">
              Indépendant · Anonyme · Sans parti pris
            </p>
            <h1 id="accueil-titre" className="text-4xl font-semibold sm:text-5xl">
              Des idées concrètes pour des finances publiques plus saines
            </h1>
            <p className="max-w-2xl text-lg text-pretty text-muted-foreground">
              Proposez anonymement une mesure pour réduire une dépense ou augmenter une recette
              publique. Une IA la vérifie, l’améliore et la chiffre en citant ses sources ; les
              idées les plus rentables et les plus faisables arrivent en tête du classement.
            </p>
            <div className="flex flex-wrap gap-3">
              <Button asChild size="lg">
                <Link to={paths.propose}>
                  <LightbulbIcon aria-hidden="true" />
                  Proposer une idée
                </Link>
              </Button>
              <Button asChild size="lg" variant="outline">
                <Link to={paths.about}>
                  Comment ça marche
                  <ArrowRightIcon aria-hidden="true" />
                </Link>
              </Button>
            </div>
          </div>
          <div>
            <h2 className="sr-only">Les quatre étapes</h2>
            <ProcessSteps />
          </div>
        </Container>
      </section>

      <Container className="space-y-10 py-10 sm:py-12">
        <PublicStats />
        <section aria-labelledby="classement-titre" className="space-y-6">
          <div className="space-y-2">
            <h2 id="classement-titre" className="text-2xl font-semibold sm:text-3xl">
              Classement des idées
            </h2>
            <p className="max-w-3xl text-pretty text-muted-foreground">
              Par défaut, les idées sont triées par rentabilité : le gain net annuel estimé, pondéré
              par la faisabilité, la qualité des données et le délai avant les premiers effets.{" "}
              <Link
                to={paths.methodology}
                className="font-medium text-primary underline underline-offset-4 hover:no-underline"
              >
                Voir le détail du score
              </Link>
            </p>
          </div>

          <IdeaFiltersBar categories={categories} />
          <IdeaRanking filters={filters} />
        </section>
      </Container>
    </>
  );
}
