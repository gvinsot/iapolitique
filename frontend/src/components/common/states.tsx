import { AlertTriangleIcon, InboxIcon, type LucideIcon, RotateCwIcon } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";

interface EmptyStateProps {
  title: string;
  description?: ReactNode;
  icon?: LucideIcon;
  action?: ReactNode;
  className?: string;
  /** Heading level, to keep the document outline valid where the state is rendered. */
  headingLevel?: 2 | 3;
}

/** Nothing to show yet (empty list, no result for a filter...). */
export function EmptyState({
  title,
  description,
  icon: Icon = InboxIcon,
  action,
  className,
  headingLevel = 2,
}: EmptyStateProps) {
  const Heading = headingLevel === 2 ? "h2" : "h3";
  return (
    <div
      className={cn(
        "flex flex-col items-center gap-3 rounded-lg border border-dashed px-6 py-12 text-center",
        className,
      )}
    >
      <Icon className="size-10 text-muted-foreground" aria-hidden="true" />
      <Heading className="text-lg font-semibold">{title}</Heading>
      {description ? (
        <div className="max-w-md text-sm text-pretty text-muted-foreground">{description}</div>
      ) : null}
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}

interface ErrorStateProps {
  title?: string;
  message: string;
  onRetry?: () => void;
  className?: string;
}

/** A request failed. Announced to screen readers when it appears. */
export function ErrorState({
  title = "Une erreur est survenue",
  message,
  onRetry,
  className,
}: ErrorStateProps) {
  return (
    <div
      role="alert"
      className={cn(
        "flex flex-col items-center gap-3 rounded-lg border border-destructive/40 px-6 py-10 text-center",
        className,
      )}
    >
      <AlertTriangleIcon className="size-10 text-destructive" aria-hidden="true" />
      <p className="text-lg font-semibold">{title}</p>
      <p className="max-w-md text-sm text-pretty text-muted-foreground">{message}</p>
      {onRetry ? (
        <Button type="button" variant="outline" onClick={onRetry} className="mt-2">
          <RotateCwIcon aria-hidden="true" />
          Réessayer
        </Button>
      ) : null}
    </div>
  );
}

/** Placeholder list while data loads; the label is read by screen readers. */
export function ListSkeleton({
  rows = 3,
  label = "Chargement en cours…",
}: {
  rows?: number;
  label?: string;
}) {
  return (
    <div role="status" aria-live="polite" className="space-y-4">
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="space-y-3 rounded-lg border p-5" aria-hidden="true">
          <Skeleton className="h-5 w-2/3" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-4 w-5/6" />
          <div className="flex gap-2 pt-1">
            <Skeleton className="h-6 w-20 rounded-full" />
            <Skeleton className="h-6 w-24 rounded-full" />
          </div>
        </div>
      ))}
    </div>
  );
}
