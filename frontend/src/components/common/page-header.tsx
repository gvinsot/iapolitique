import type { ReactNode } from "react";

import { Container } from "@/components/layout/container";
import { cn } from "@/lib/utils";

interface PageHeaderProps {
  title: string;
  /** Short label above the title (section name). */
  eyebrow?: string;
  description?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

/** Page heading block: the only <h1> of a page. */
export function PageHeader({ title, eyebrow, description, actions, className }: PageHeaderProps) {
  return (
    <div className={cn("border-b bg-muted/30", className)}>
      <Container className="flex flex-col gap-4 py-8 sm:py-10 md:flex-row md:items-end md:justify-between">
        <div className="max-w-3xl space-y-3">
          {eyebrow ? (
            <p className="text-sm font-semibold tracking-wide text-primary uppercase">{eyebrow}</p>
          ) : null}
          <h1 className="text-3xl font-semibold sm:text-4xl">{title}</h1>
          {description ? (
            <div className="text-base text-pretty text-muted-foreground sm:text-lg">
              {description}
            </div>
          ) : null}
        </div>
        {actions ? <div className="flex shrink-0 flex-wrap gap-2">{actions}</div> : null}
      </Container>
    </div>
  );
}
