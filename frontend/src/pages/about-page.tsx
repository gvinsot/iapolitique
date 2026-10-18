import {
  EyeIcon,
  type LucideIcon,
  ScaleIcon,
  SearchCheckIcon,
  TargetIcon,
  UserRoundXIcon,
} from "lucide-react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { PageHeader } from "@/components/common/page-header";
import { ProcessSteps } from "@/components/common/process-steps";
import { Container } from "@/components/layout/container";
import { Button } from "@/components/ui/button";
import { useDocumentTitle } from "@/hooks/use-document-title";

interface Principle {
  icon: LucideIcon;
  title: string;
  text: string;
}

const principles: readonly Principle[] = [
  {
    icon: ScaleIcon,
    title: "Apolitique",
    text: "Aucune étiquette, aucun parti, aucun élu nommé. Les effets d’une mesure sont décrits par public concerné, jamais par camp.",
  },
  {
    icon: UserRoundXIcon,
    title: "Anonyme",
    text: "Ni compte, ni e-mail, ni adresse IP conservée, ni cookie, ni traceur. Le suivi passe par un lien privé.",
  },
  {
    icon: EyeIcon,
    title: "Transparent",
    text: "Les prompts, la grille de filtrage et la formule de score sont publics et versionnés. Chaque analyse indique les versions utilisées.",
  },
  {
    icon: SearchCheckIcon,
    title: "Vérifiable",
    text: "Chaque chiffrage cite ses sources, expose ses hypothèses et donne une fourchette plutôt qu’un chiffre unique.",
  },
  {
    icon: TargetIcon,
    title: "Utile",
    text: "Le classement favorise les idées crédibles et faisables, pas les montants spectaculaires.",
  },
];

export function AboutPage() {
  useDocumentTitle("À propos");

  return (
    <>
      <PageHeader
        eyebrow="Fonctionnement"
        title="À propos d’iapolitique"
        description="Un lieu pour proposer, sans se déclarer et sans parti pris, des mesures concrètes qui améliorent l’état des finances publiques, et pour les voir examinées sérieusement."
      />

      <Container className="space-y-14 py-10 sm:py-12">
        <section aria-labelledby="pourquoi" className="prose-content">
          <h2 id="pourquoi">Pourquoi ce site ?</h2>
          <p>
            Beaucoup d’idées pour mieux dépenser l’argent public ou trouver des recettes justes
            existent, mais elles sont rarement chiffrées, rarement comparées entre elles, et souvent
            rangées d’emblée dans un camp politique. iapolitique propose une autre démarche : chacun
            peut déposer une idée de façon anonyme, l’intelligence artificielle l’examine selon des
            règles publiques identiques pour tous, et les idées sont comparées sur un critère
            commun, leur rentabilité pour les finances publiques.
          </p>
          <p>
            Sont concernées toutes les administrations publiques françaises : l’État, la sécurité
            sociale et les collectivités territoriales (régions, départements, intercommunalités,
            communes).
          </p>
        </section>

        <section aria-labelledby="etapes" className="space-y-6">
          <div className="max-w-prose space-y-2">
            <h2 id="etapes" className="text-xl font-semibold">
              Le parcours d’une idée, en quatre étapes
            </h2>
            <p className="text-muted-foreground">
              Dépôt anonyme, filtre par IA, analyse par Mistral, puis classement.
            </p>
          </div>
          <ProcessSteps detailed className="md:grid-cols-2" />
        </section>

        <section aria-labelledby="principes" className="space-y-6">
          <h2 id="principes" className="text-xl font-semibold">
            Nos principes
          </h2>
          <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {principles.map((principle) => (
              <li key={principle.title} className="space-y-2 rounded-lg border bg-card p-5">
                <principle.icon className="size-6 text-primary" aria-hidden="true" />
                <h3 className="font-semibold">{principle.title}</h3>
                <p className="text-sm text-pretty text-muted-foreground">{principle.text}</p>
              </li>
            ))}
          </ul>
        </section>

        <section aria-labelledby="limites" className="prose-content">
          <h2 id="limites">Ce que ce site n’est pas</h2>
          <ul>
            <li>
              <strong>Ni un site officiel</strong> : il n’a aucun lien avec l’État, une
              administration ou un parti politique, et les idées publiées n’engagent aucune
              autorité.
            </li>
            <li>
              <strong>Ni un sondage, ni un vote</strong> : il n’y a ni compte ni « j’aime ». Une
              idée proposée plusieurs fois est simplement regroupée avec ses doublons.
            </li>
            <li>
              <strong>Ni un oracle</strong> : les analyses sont générées par IA et peuvent comporter
              des erreurs. Elles indiquent leur niveau de confiance et leurs sources pour que chacun
              puisse les vérifier. Un bouton permet de signaler une erreur.
            </li>
          </ul>
        </section>

        <div className="flex flex-wrap gap-3">
          <Button asChild size="lg">
            <Link to={paths.propose}>Proposer une idée</Link>
          </Button>
          <Button asChild size="lg" variant="outline">
            <Link to={paths.methodology}>Lire la méthodologie</Link>
          </Button>
        </div>
      </Container>
    </>
  );
}
