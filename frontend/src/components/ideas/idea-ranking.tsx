import { DownloadIcon, SearchXIcon, SparklesIcon } from "lucide-react";
import { Link } from "react-router";

import { errorMessage } from "@/api/errors";
import { paths } from "@/app/paths";
import { Pagination } from "@/components/common/pagination";
import { EmptyState, ErrorState, ListSkeleton } from "@/components/common/states";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useIdeaList } from "@/hooks/use-idea-list";
import { EXPORT_URLS } from "@/lib/exports";
import { formatInteger } from "@/lib/format";
import {
  defaultIdeaFilters,
  hasActiveIdeaFilters,
  type IdeaFilters,
  ideaFiltersHref,
} from "@/lib/idea-filters";
import { cn } from "@/lib/utils";

import { AiGeneratedNotice } from "./ai-generated-notice";
import { IdeaCard } from "./idea-card";

const linkClass = "font-medium text-primary underline underline-offset-4 hover:no-underline";

function resultCount(total: number): string {
  if (total === 0) return "Aucune idée";
  return total === 1 ? "1 idée" : `${formatInteger(total)} idées`;
}

interface IdeaRankingProps {
  filters: IdeaFilters;
}

/**
 * Results of the ranking for the filters of the URL (GET /api/ideas): AI notice, cards,
 * pagination, and explicit loading, error and empty states.
 */
export function IdeaRanking({ filters }: IdeaRankingProps) {
  const query = useIdeaList(filters);
  const filtered = hasActiveIdeaFilters(filters);

  if (query.isPending) return <ListSkeleton rows={3} label="Chargement du classement…" />;

  if (query.isError) {
    return (
      <ErrorState
        title="Le classement n’a pas pu être chargé"
        message={errorMessage(query.error)}
        onRetry={() => {
          void query.refetch();
        }}
      />
    );
  }

  const list = query.data;
  if (list.total === 0) {
    return filtered ? (
      <EmptyState
        headingLevel={3}
        icon={SearchXIcon}
        title="Aucune idée ne correspond à ces critères"
        description="Essayez d’autres mots, un autre territoire ou retirez un filtre."
        action={
          <Button asChild variant="outline">
            <Link to={ideaFiltersHref(defaultIdeaFilters, 1)}>Voir toutes les idées</Link>
          </Button>
        }
      />
    ) : (
      <EmptyState
        headingLevel={3}
        title="Aucune idée publiée pour le moment"
        description="Les idées apparaîtront ici dès que leur analyse sera terminée. Vous pouvez être parmi les premiers à proposer une mesure."
        action={
          <Button asChild>
            <Link to={paths.propose}>Proposer une idée</Link>
          </Button>
        }
      />
    );
  }

  if (list.items.length === 0) {
    // A shared link to a page that no longer exists (fewer ideas than before).
    return (
      <EmptyState
        headingLevel={3}
        title="Cette page du classement est vide"
        description={`Le classement compte ${resultCount(list.total).toLowerCase()} sur ${list.total_pages} page${list.total_pages > 1 ? "s" : ""}.`}
        action={
          <Button asChild variant="outline">
            <Link to={ideaFiltersHref(filters, 1)}>Revenir à la première page</Link>
          </Button>
        }
      />
    );
  }

  const ranked = filters.sort === "score";
  const firstRank = (list.page - 1) * list.per_page + 1;

  return (
    <div className={cn("space-y-6", query.isPlaceholderData && "opacity-60 transition-opacity")}>
      <p role="status" className="text-sm text-muted-foreground">
        <strong className="text-foreground">{resultCount(list.total)}</strong>
        {list.total_pages > 1 ? ` · page ${list.page} sur ${list.total_pages}` : null}
      </p>
      {list.search === "approximate" ? (
        <Alert variant="info">
          <SparklesIcon aria-hidden="true" />
          <AlertDescription>
            <p>
              Aucune idée ne contient exactement « {filters.q} » : voici des résultats approchants.
            </p>
          </AlertDescription>
        </Alert>
      ) : null}
      <AiGeneratedNotice />
      <ol
        aria-label="Idées classées"
        aria-busy={query.isFetching}
        className="grid gap-4 md:grid-cols-2"
      >
        {list.items.map((idea, index) => (
          <li key={idea.slug} className="flex">
            <IdeaCard
              idea={idea}
              rank={ranked ? firstRank + index : undefined}
              className="w-full"
            />
          </li>
        ))}
      </ol>
      <Pagination
        page={list.page}
        pageCount={list.total_pages}
        hrefForPage={(page) => ideaFiltersHref(filters, page)}
      />
      <OpenDataLinks />
    </div>
  );
}

/** Download links of the open data files, with their licence. */
export function OpenDataLinks({ className }: { className?: string }) {
  return (
    <p
      className={cn(
        "flex flex-wrap items-center gap-x-2 gap-y-1 border-t pt-4 text-sm text-muted-foreground",
        className,
      )}
    >
      <DownloadIcon className="size-4" aria-hidden="true" />
      <span>Données ouvertes :</span>
      <a href={EXPORT_URLS.json} download className={linkClass}>
        toutes les idées en JSON
      </a>
      <span aria-hidden="true">·</span>
      <a href={EXPORT_URLS.csv} download className={linkClass}>
        en CSV
      </a>
      <span aria-hidden="true">·</span>
      <Link to={paths.licence} className={linkClass}>
        Licence Ouverte 2.0
      </Link>
    </p>
  );
}
