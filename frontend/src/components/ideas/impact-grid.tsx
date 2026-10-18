import {
  CircleHelpIcon,
  CircleMinusIcon,
  CirclePlusIcon,
  CircleSlashIcon,
  type LucideIcon,
} from "lucide-react";

import type { Impact } from "@/lib/idea-model";
import { type ImpactEffect, impactEffectLabels } from "@/lib/labels";
import { cn } from "@/lib/utils";

import { CitedText } from "./cited-text";

/**
 * Each effect has its own shape and word; colours only reinforce them and avoid the red/blue
 * pair associated with political camps: teal for positive, ochre for negative, greys otherwise
 * (readable with any colour vision deficiency).
 */
const effectStyles: Record<ImpactEffect, { icon: LucideIcon; className: string }> = {
  positive: { icon: CirclePlusIcon, className: "text-primary" },
  negative: { icon: CircleMinusIcon, className: "text-warning" },
  neutral: { icon: CircleSlashIcon, className: "text-muted-foreground" },
  uncertain: { icon: CircleHelpIcon, className: "text-muted-foreground" },
};

interface ImpactGridProps {
  impacts: Impact[];
  sourceIds: ReadonlySet<string>;
  className?: string;
}

/** Expected effect of the measure for every public of the taxonomy, described by public. */
export function ImpactGrid({ impacts, sourceIds, className }: ImpactGridProps) {
  return (
    <ul className={cn("grid gap-3 sm:grid-cols-2 lg:grid-cols-3", className)}>
      {impacts.map((impact) => {
        const style = effectStyles[impact.effect];
        return (
          <li key={impact.public} className="space-y-2 rounded-lg border bg-card p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="font-medium">{impact.label}</p>
              <p className={cn("flex items-center gap-1.5 text-sm font-semibold", style.className)}>
                <style.icon className="size-4" aria-hidden="true" />
                <span className="sr-only">Effet : </span>
                {impactEffectLabels[impact.effect]}
              </p>
            </div>
            <p className="text-sm text-pretty text-muted-foreground">
              <CitedText text={impact.explanation} sourceIds={sourceIds} />
            </p>
          </li>
        );
      })}
    </ul>
  );
}
