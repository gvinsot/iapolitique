import { CircleMinusIcon, CirclePlusIcon, type LucideIcon, TriangleAlertIcon } from "lucide-react";
import { useId } from "react";

import { cn } from "@/lib/utils";

import { CitedText } from "./cited-text";

interface TextListProps {
  items: string[];
  sourceIds: ReadonlySet<string>;
  empty: string;
  icon?: LucideIcon;
  ordered?: boolean;
}

function TextList({ items, sourceIds, empty, icon: Icon, ordered = false }: TextListProps) {
  if (items.length === 0) return <p className="text-sm text-muted-foreground">{empty}</p>;
  const List = ordered ? "ol" : "ul";
  return (
    <List className={cn("space-y-2", ordered && "list-decimal pl-5")}>
      {items.map((item, index) => (
        <li key={index} className={cn(Icon && "flex gap-2")}>
          {Icon ? (
            <Icon className="mt-1 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          ) : null}
          <span className="text-pretty">
            <CitedText text={item} sourceIds={sourceIds} />
          </span>
        </li>
      ))}
    </List>
  );
}

interface ProsConsProps {
  pros: string[];
  cons: string[];
  risks?: string[];
  sourceIds: ReadonlySet<string>;
  headingLevel?: 3 | 4;
}

/**
 * Arguments for and against, side by side with the same visual weight (neutrality, AGENTS.md
 * §6.2). Shapes and words carry the meaning, not colours.
 */
export function ProsCons({ pros, cons, risks = [], sourceIds, headingLevel = 3 }: ProsConsProps) {
  const Heading = headingLevel === 3 ? "h3" : "h4";
  const prosId = useId();
  const consId = useId();
  const risksId = useId();
  return (
    <div className="space-y-6">
      <div className="grid gap-4 md:grid-cols-2">
        <section aria-labelledby={prosId} className="space-y-3 rounded-lg border bg-card p-4">
          <Heading id={prosId} className="flex items-center gap-2 font-semibold">
            <CirclePlusIcon className="size-4" aria-hidden="true" />
            Avantages
          </Heading>
          <TextList items={pros} sourceIds={sourceIds} empty="Aucun avantage identifié." />
        </section>
        <section aria-labelledby={consId} className="space-y-3 rounded-lg border bg-card p-4">
          <Heading id={consId} className="flex items-center gap-2 font-semibold">
            <CircleMinusIcon className="size-4" aria-hidden="true" />
            Inconvénients
          </Heading>
          <TextList items={cons} sourceIds={sourceIds} empty="Aucun inconvénient identifié." />
        </section>
      </div>
      {risks.length > 0 ? (
        <section aria-labelledby={risksId} className="space-y-3 rounded-lg border bg-card p-4">
          <Heading id={risksId} className="flex items-center gap-2 font-semibold">
            <TriangleAlertIcon className="size-4" aria-hidden="true" />
            Risques
          </Heading>
          <TextList items={risks} sourceIds={sourceIds} empty="" />
        </section>
      ) : null}
    </div>
  );
}

/** Explicit calculation assumptions of an estimate. */
export function AssumptionsList({
  assumptions,
  sourceIds,
}: {
  assumptions: string[];
  sourceIds: ReadonlySet<string>;
}) {
  return (
    <TextList
      items={assumptions}
      sourceIds={sourceIds}
      empty="Aucune hypothèse n’est explicitée."
      ordered
    />
  );
}
