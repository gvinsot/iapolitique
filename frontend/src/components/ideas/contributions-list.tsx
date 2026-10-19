import { CheckIcon, MinusIcon } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { formatDate } from "@/lib/format";
import type { ContributionConsidered } from "@/lib/idea-model";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";
import { labelOf } from "@/lib/taxonomy-labels";

interface ContributionsListProps {
  contributions: ContributionConsidered[];
  /** Contributions received in total, integrated or not (AGENTS.md §5.5). */
  totalCount?: number;
}

/**
 * Contributions taken into account by the current analysis. Only their neutral summary and the
 * answer of the analysis are shown, never the raw text of the visitor (AGENTS.md §5.5).
 */
export function ContributionsList({ contributions, totalCount }: ContributionsListProps) {
  const labels = useTaxonomyLabels();
  const count = totalCount ?? contributions.length;
  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        {count === 0
          ? "Aucune contribution n’a encore été reçue pour cette idée."
          : count === 1
            ? "1 contribution reçue."
            : `${count} contributions reçues.`}{" "}
        Seuls les résumés neutres des contributions examinées par l’analyse sont affichés.
      </p>
      {contributions.length > 0 ? (
        <ul className="space-y-3">
          {contributions.map((contribution, index) => (
            // The API sends no identifier (AGENTS.md §13.2): the order is stable for a page.
            <li key={index} className="space-y-2 rounded-lg border bg-card p-4">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant="secondary">
                  {labelOf(labels.contributionKinds, contribution.kind)}
                </Badge>
                <Badge variant="outline">
                  {contribution.taken_into_account ? (
                    <CheckIcon aria-hidden="true" />
                  ) : (
                    <MinusIcon aria-hidden="true" />
                  )}
                  {contribution.taken_into_account ? "Prise en compte" : "Non retenue"}
                </Badge>
                {contribution.integrated_on ? (
                  <span className="text-xs text-muted-foreground">
                    Examinée le {formatDate(contribution.integrated_on)}
                  </span>
                ) : null}
              </div>
              <p className="text-pretty">{contribution.summary}</p>
              {contribution.response ? (
                <p className="text-sm text-pretty text-muted-foreground">
                  <span className="font-medium text-foreground">Réponse de l’analyse : </span>
                  {contribution.response}
                </p>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
