import {
  FileSearchIcon,
  ListOrderedIcon,
  type LucideIcon,
  PenLineIcon,
  ShieldCheckIcon,
} from "lucide-react";

import { cn } from "@/lib/utils";

interface ProcessStep {
  icon: LucideIcon;
  title: string;
  summary: string;
  detail: string;
}

const processSteps: readonly ProcessStep[] = [
  {
    icon: PenLineIcon,
    title: "Dépôt anonyme",
    summary: "Vous décrivez votre idée en quelques lignes, sans compte ni adresse e-mail.",
    detail:
      "Aucune donnée personnelle n’est demandée ni conservée : ni nom, ni adresse e-mail, ni adresse IP. Vous recevez un lien de suivi personnel, affiché une seule fois, pour connaître le devenir de votre proposition.",
  },
  {
    icon: ShieldCheckIcon,
    title: "Filtre par IA",
    summary:
      "En quelques secondes, un modèle vérifie que l’idée est respectueuse, dans le sujet et réaliste.",
    detail:
      "Un modèle de décision évalue chaque proposition selon une grille publique : respect des personnes, absence de ciblage d’un parti, d’un élu ou d’un groupe, lien avec les finances publiques, compatibilité avec les droits fondamentaux et réalisme. En cas de doute, un second avis est demandé à un autre modèle. Si l’idée est refusée, le motif vous est donné pour que vous puissiez la reformuler.",
  },
  {
    icon: FileSearchIcon,
    title: "Analyse par Mistral",
    summary:
      "L’idée est reformulée de façon neutre, ses avantages et inconvénients sont pesés, son gain est chiffré avec des sources.",
    detail:
      "Le modèle Mistral recherche des sources publiques (Cour des comptes, INSEE, lois de finances, rapports parlementaires…), propose une version améliorée de l’idée, estime une fourchette de gain annuel et de coût de mise en œuvre, et décrit les effets attendus pour chaque catégorie de public, sans jamais qualifier politiquement la mesure.",
  },
  {
    icon: ListOrderedIcon,
    title: "Classement",
    summary: "Les idées publiées sont classées selon un score de rentabilité transparent.",
    detail:
      "Le score combine le gain net annuel estimé, la faisabilité, la qualité des données disponibles et le délai avant les premiers effets. Ses composantes sont affichées sur chaque idée et la formule est publiée sur la page Méthodologie : aucune note n’est ajustée à la main.",
  },
];

interface ProcessStepsProps {
  /** Shows the long explanation of each step (about page) instead of the summary. */
  detailed?: boolean;
  className?: string;
  headingLevel?: 2 | 3;
}

export function ProcessSteps({ detailed = false, className, headingLevel = 3 }: ProcessStepsProps) {
  const Heading = headingLevel === 2 ? "h2" : "h3";
  return (
    <ol className={cn("grid gap-4", className)}>
      {processSteps.map((step, index) => (
        <li key={step.title} className="flex gap-4 rounded-lg border bg-card p-4 sm:p-5">
          <span
            className="flex size-10 shrink-0 items-center justify-center rounded-full bg-accent text-accent-foreground"
            aria-hidden="true"
          >
            <step.icon className="size-5" />
          </span>
          <div className="space-y-1">
            <Heading className="font-semibold">
              <span className="text-muted-foreground">{index + 1}. </span>
              {step.title}
            </Heading>
            <p className="text-sm text-pretty text-muted-foreground">
              {detailed ? step.detail : step.summary}
            </p>
          </div>
        </li>
      ))}
    </ol>
  );
}
