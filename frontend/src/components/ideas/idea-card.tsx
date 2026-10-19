import { BotIcon, CalculatorIcon, ClockIcon, UsersIcon } from "lucide-react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { formatMonths } from "@/lib/format";
import type { IdeaSummary } from "@/lib/idea-model";
import { cn } from "@/lib/utils";

import {
  AdminLevelBadge,
  CategoryBadge,
  ConfidenceBadge,
  FeasibilityBadge,
  KindBadge,
  TerritoryBadge,
} from "./idea-badges";
import { GainRange } from "./gain-range";
import { SCORE_LABEL, ScorePoints } from "./score-breakdown";

interface IdeaCardProps {
  idea: IdeaSummary;
  /** Position in the current ranking, shown when the list is sorted by score. */
  rank?: number;
  headingLevel?: 2 | 3;
  className?: string;
}

function proposalCountLabel(count: number): string {
  return count <= 1 ? "Proposée 1 fois" : `Proposée ${count} fois`;
}

/**
 * An idea of the ranking. Title and summary are AI reformulations: the card is marked as
 * generated (`data-ai-generated`) and says so; the list shows the full notice above the cards.
 */
export function IdeaCard({ idea, rank, headingLevel = 3, className }: IdeaCardProps) {
  const Heading = headingLevel === 2 ? "h2" : "h3";
  return (
    <article
      data-ai-generated="true"
      className={cn(
        "relative flex flex-col gap-4 rounded-lg border bg-card p-4 transition-colors focus-within:ring-2 focus-within:ring-ring hover:border-primary/50 sm:p-5",
        className,
      )}
    >
      <div className="flex flex-wrap gap-2">
        <AdminLevelBadge level={idea.admin_level} />
        {idea.territory?.name ? <TerritoryBadge territory={idea.territory} /> : null}
        <CategoryBadge category={idea.category} />
        <KindBadge kind={idea.kind} />
      </div>

      <div className="space-y-2">
        <Heading className="text-lg leading-snug font-semibold text-pretty">
          {rank !== undefined ? (
            <span className="mr-2 text-muted-foreground tabular-nums">{rank}.</span>
          ) : null}
          {/* The link covers the whole card; other content stays selectable above it. */}
          <Link
            to={paths.idea(idea.slug)}
            className="outline-none after:absolute after:inset-0 after:content-[''] hover:underline"
          >
            {idea.title}
          </Link>
        </Heading>
        <p className="line-clamp-3 text-sm text-pretty text-muted-foreground">{idea.summary}</p>
      </div>

      <GainRange range={idea.annual_gain_eur} label="Gain annuel estimé" compact />

      <div className="flex flex-wrap items-center gap-2">
        <FeasibilityBadge score={idea.feasibility} />
        <ConfidenceBadge confidence={idea.confidence} />
        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
          <ClockIcon className="size-3.5" aria-hidden="true" />
          Premiers effets : {formatMonths(idea.time_to_effect_months)}
        </span>
      </div>

      <div className="mt-auto flex flex-wrap items-center justify-between gap-2 border-t pt-3 text-sm">
        <p className="flex items-center gap-1.5">
          <CalculatorIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <span className="text-muted-foreground">{SCORE_LABEL} :</span>
          <ScorePoints points={idea.points} />
        </p>
        <p className="flex items-center gap-1.5 text-muted-foreground">
          <UsersIcon className="size-4" aria-hidden="true" />
          {proposalCountLabel(idea.proposal_count)}
        </p>
      </div>
      <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <BotIcon className="size-3.5" aria-hidden="true" />
        Titre, résumé et chiffrage générés par IA (Mistral AI), sans relecture humaine.
      </p>
    </article>
  );
}
