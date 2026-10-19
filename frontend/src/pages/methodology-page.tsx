import { Link } from "react-router";

import { $api } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { paths } from "@/app/paths";
import { PageHeader } from "@/components/common/page-header";
import { ErrorState, ListSkeleton } from "@/components/common/states";
import { Container } from "@/components/layout/container";
import { MethodologyInForce } from "@/components/methodology/methodology-in-force";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { usePageDescription } from "@/hooks/use-page-meta";
import { RULES_SCORE_ANCHOR } from "@/lib/rules";

const tableOfContents = [
  ["filtrage", "Le filtrage"],
  ["analyse", "L’analyse et le chiffrage"],
  ["score", "Le score de rentabilité"],
  ["neutralite", "La neutralité"],
  ["versions", "Les versions"],
  ["production", "Textes en vigueur"],
] as const;

/** Texts in force, read from the API (AGENTS.md §6.5): never copied into the site. */
function TextsInForce() {
  const query = $api.useQuery("get", "/api/methodology", {}, { staleTime: 5 * 60_000 });
  if (query.isPending) return <ListSkeleton rows={2} label="Chargement des textes en vigueur…" />;
  if (query.isError) {
    return (
      <ErrorState
        title="Les textes en vigueur n’ont pas pu être chargés"
        message={errorMessage(query.error)}
        onRetry={() => {
          void query.refetch();
        }}
      />
    );
  }
  return <MethodologyInForce methodology={query.data} />;
}

export function MethodologyPage() {
  useDocumentTitle("Méthodologie");
  usePageDescription(
    "Prompts, grille de filtrage, seuils et formule du score d’iapolitique, tels qu’utilisés en production et versionnés.",
  );

  return (
    <>
      <PageHeader
        eyebrow="Transparence"
        title="Méthodologie"
        description="Comment les idées sont filtrées, analysées, chiffrées et classées. Les prompts, la grille de filtrage et la formule de score sont publics et versionnés."
      />
      <Container className="grid gap-10 py-10 lg:grid-cols-[minmax(0,1fr)_16rem]">
        <article className="prose-content">
          <h2 id="filtrage">1. Le filtrage</h2>
          <p>
            Chaque proposition est évaluée par un <strong>modèle de décision</strong>, un modèle
            d’intelligence artificielle spécialisé qui répond à une série de questions fermées et
            donne, pour chacune, une probabilité. Il vérifie notamment que l’idée :
          </p>
          <ul>
            <li>ne cherche pas à manipuler une intelligence artificielle ;</li>
            <li>est respectueuse (ni insulte, ni haine, ni incitation à la violence) ;</li>
            <li>
              ne vise pas nommément un parti, un élu, une personne ou un groupe, et ne fait campagne
              ni pour ni contre un candidat ou un parti ;
            </li>
            <li>ne permet de reconnaître aucun particulier (nom, signature, poste précis) ;</li>
            <li>
              propose une mesure qui réduit une dépense ou augmente une recette d’une administration
              publique française ;
            </li>
            <li>est compatible avec les droits fondamentaux ;</li>
            <li>est réaliste à mettre en œuvre.</li>
          </ul>
          <p>
            Chaque critère dispose de deux seuils : en dessous du premier, l’idée est refusée ;
            au-dessus du second, elle est acceptée. Entre les deux, dans la « zone grise », un
            second avis est demandé à un autre modèle avec la même grille. Le motif d’un refus est
            toujours communiqué à l’auteur.
          </p>

          <h2 id="analyse">2. L’analyse et le chiffrage</h2>
          <p>
            Les idées retenues sont analysées par un modèle de <strong>Mistral AI</strong>, avec une
            recherche web obligatoire. L’analyse comprend une reformulation neutre, une version
            améliorée de la proposition, les avantages, les inconvénients et les risques, les effets
            attendus pour chaque catégorie de public, ainsi qu’une estimation en fourchette (basse,
            médiane, haute) du gain annuel et du coût de mise en œuvre.
          </p>
          <p>
            Tout chiffre clé est accompagné d’au moins une source, de préférence publique : Cour des
            comptes, INSEE, projets de loi de finances et de financement de la sécurité sociale,
            DREES, rapports parlementaires, comptes des collectivités. Les hypothèses de calcul sont
            explicitées. Les sources affichées sont uniquement des pages réellement consultées par
            la recherche web : leur titre et leur adresse sont repris de cette recherche, jamais
            rédigés par l’IA. Une recherche web qui ne renvoie aucune page est relancée : sans page
            consultée, aucune analyse n’est publiée.
          </p>

          <h2 id="score">3. Le score de rentabilité</h2>
          <p>Le classement repose sur un score composé, calculé automatiquement à partir :</p>
          <ul>
            <li>du gain net annuel estimé (le coût de mise en œuvre étant amorti sur 5 ans) ;</li>
            <li>de la faisabilité, notée de 1 à 5 ;</li>
            <li>de la confiance dans les données disponibles ;</li>
            <li>du délai avant les premiers effets.</li>
          </ul>
          <p>
            Ainsi, une idée crédible et rapide à mettre en œuvre peut devancer une idée au montant
            spectaculaire mais incertain. Aucun score n’est ajusté à la main. Les composantes du
            score sont affichées sur chaque idée, et le score est ramené sur 1000 points par une
            échelle fixe (125 points par facteur 10, voir la formule ci-dessous).
          </p>

          <h2 id="neutralite">4. La neutralité</h2>
          <p>
            Les consignes données aux modèles interdisent toute qualification politique (gauche,
            droite, libéral, social…) et toute mention d’un parti ou d’un élu. Les arguments pour et
            contre sont présentés avec le même niveau de détail, et les effets sont décrits par
            public concerné, jamais par camp.
          </p>

          <h2 id="versions">5. Les versions</h2>
          <p>
            Chaque prompt, la grille de filtrage et la formule de score portent un numéro de
            version. Chaque décision et chaque analyse enregistrent les versions utilisées, afin que
            tout résultat puisse être retracé.
          </p>

          <h2 id="production">6. Textes en vigueur</h2>
          <p>
            Les textes ci-dessous sont lus en direct depuis le service : ce sont exactement ceux
            qu’utilisent le filtrage et l’analyse, avec leurs versions. Le détail des paramètres du
            score figure aussi dans les{" "}
            <Link to={`${paths.terms}#${RULES_SCORE_ANCHOR}`}>règles du site</Link>.
          </p>
          <TextsInForce />
        </article>

        <nav aria-label="Sommaire" className="hidden lg:block">
          <div className="sticky top-24 space-y-2 text-sm">
            <p className="font-semibold">Sommaire</p>
            <ul className="space-y-1 text-muted-foreground">
              {tableOfContents.map(([anchor, label]) => (
                <li key={anchor}>
                  <a href={`#${anchor}`} className="rounded-sm hover:text-foreground">
                    {label}
                  </a>
                </li>
              ))}
            </ul>
          </div>
        </nav>
      </Container>
    </>
  );
}
