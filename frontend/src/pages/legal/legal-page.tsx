import type { ReactNode } from "react";

import { PageHeader } from "@/components/common/page-header";
import { LegalDetailsUnavailable } from "@/components/legal/legal-details";
import { Container } from "@/components/layout/container";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { usePageDescription } from "@/hooks/use-page-meta";
import { type LegalDetails } from "@/hooks/use-legal-info";
import { formatLegalDate, LEGAL_TEXTS_DATES, type LegalPageKey } from "@/lib/legal";

export interface TocEntry {
  anchor: string;
  label: string;
}

interface LegalPageProps {
  title: string;
  description: string;
  pageKey: LegalPageKey;
  details: LegalDetails;
  toc?: readonly TocEntry[];
  children: ReactNode;
}

/**
 * Shell of the legal pages (AGENTS.md §7.5): heading, date of last update (from GET /api/legal,
 * or the date of the texts shipped with the site when the API is unreachable), optional table
 * of contents and long-form content.
 */
export function LegalPage({ title, description, pageKey, details, toc, children }: LegalPageProps) {
  useDocumentTitle(title);
  usePageDescription(description);
  const updatedAt = details.info?.pages[pageKey] ?? LEGAL_TEXTS_DATES[pageKey];

  return (
    <>
      <PageHeader
        eyebrow="Informations légales"
        title={title}
        description={
          <>
            <p>{description}</p>
            <p className="mt-2 text-sm">
              Dernière mise à jour :{" "}
              <time dateTime={updatedAt.slice(0, 10)}>{formatLegalDate(updatedAt)}</time>
            </p>
          </>
        }
      />
      <Container className="grid gap-10 py-10 lg:grid-cols-[minmax(0,1fr)_16rem]">
        <div className="min-w-0 space-y-8">
          {details.failed ? <LegalDetailsUnavailable onRetry={details.retry} /> : null}
          <article className="prose-content">{children}</article>
        </div>
        {toc && toc.length > 0 ? (
          <nav aria-label="Sommaire" className="order-first lg:order-last">
            <div className="space-y-2 rounded-lg border bg-card p-4 text-sm lg:sticky lg:top-24">
              <p className="font-semibold">Sommaire</p>
              <ol className="space-y-1 text-muted-foreground">
                {toc.map((entry) => (
                  <li key={entry.anchor}>
                    <a href={`#${entry.anchor}`} className="rounded-sm hover:text-foreground">
                      {entry.label}
                    </a>
                  </li>
                ))}
              </ol>
            </div>
          </nav>
        ) : null}
      </Container>
    </>
  );
}
