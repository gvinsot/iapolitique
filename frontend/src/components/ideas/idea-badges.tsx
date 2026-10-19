import { GaugeIcon, LandmarkIcon, MapPinIcon, ScaleIcon, TagIcon } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import type { CategoryRef, Territory } from "@/lib/idea-model";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";
import {
  type AdminLevel,
  type Confidence,
  feasibilityLabel,
  type IdeaKind,
  type LegalVehicle,
} from "@/lib/labels";
import { labelOf } from "@/lib/taxonomy-labels";
import { cn } from "@/lib/utils";

/** Badges describing an idea. Each one carries its meaning in words, not only in colour. */

export function AdminLevelBadge({ level }: { level: AdminLevel }) {
  const labels = useTaxonomyLabels();
  return (
    <Badge variant="secondary">
      <LandmarkIcon aria-hidden="true" />
      <span className="sr-only">Niveau administratif : </span>
      {labelOf(labels.adminLevels, level)}
    </Badge>
  );
}

export function TerritoryBadge({ territory }: { territory: Territory }) {
  return (
    <Badge variant="outline">
      <MapPinIcon aria-hidden="true" />
      <span className="sr-only">Territoire : </span>
      {territory.name}
    </Badge>
  );
}

export function CategoryBadge({ category }: { category: CategoryRef }) {
  return (
    <Badge variant="outline" className="max-w-full">
      <TagIcon aria-hidden="true" />
      <span className="sr-only">Catégorie : </span>
      <span className="truncate">{category.label}</span>
    </Badge>
  );
}

export function KindBadge({ kind }: { kind: IdeaKind }) {
  const labels = useTaxonomyLabels();
  return (
    <Badge variant="outline">
      <span className="sr-only">Type de mesure : </span>
      {labelOf(labels.ideaKinds, kind)}
    </Badge>
  );
}

export function LegalVehicleBadge({ vehicle }: { vehicle: LegalVehicle }) {
  const labels = useTaxonomyLabels();
  return (
    <Badge variant="outline" className="max-w-full">
      <ScaleIcon aria-hidden="true" />
      <span className="truncate">
        <span className="sr-only">Texte nécessaire : </span>
        {labelOf(labels.legalVehicles, vehicle)}
      </span>
    </Badge>
  );
}

const FEASIBILITY_STEPS = [1, 2, 3, 4, 5] as const;

/** Feasibility 1 to 5: five pips plus the score and its label in words. */
export function FeasibilityBadge({ score, className }: { score: number; className?: string }) {
  const value = Math.min(5, Math.max(1, Math.round(score)));
  return (
    <Badge variant="outline" className={cn("gap-1.5", className)}>
      <span className="flex gap-0.5" aria-hidden="true">
        {FEASIBILITY_STEPS.map((step) => (
          <span
            key={step}
            className={cn(
              "size-1.5 rounded-full border border-foreground/60",
              step <= value && "bg-foreground/80",
            )}
          />
        ))}
      </span>
      Faisabilité {value}/5 · {feasibilityLabel(value)}
    </Badge>
  );
}

export function ConfidenceBadge({ confidence }: { confidence: Confidence }) {
  const labels = useTaxonomyLabels();
  return (
    <Badge variant="outline">
      <GaugeIcon aria-hidden="true" />
      Confiance {labelOf(labels.confidenceLevels, confidence).toLowerCase()}
    </Badge>
  );
}
