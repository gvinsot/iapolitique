import { Loader2Icon } from "lucide-react";

/** Shown while a page chunk loads (first visit of a lazily loaded route). */
export function PageLoading() {
  return (
    <div role="status" className="flex min-h-[50dvh] items-center justify-center gap-3 p-8">
      <Loader2Icon className="size-5 animate-spin text-muted-foreground" aria-hidden="true" />
      <span className="text-muted-foreground">Chargement…</span>
    </div>
  );
}
