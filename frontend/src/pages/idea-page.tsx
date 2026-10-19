import { ArrowLeftIcon, SearchXIcon } from "lucide-react";
import { useEffect } from "react";
import { Link, useNavigate, useParams } from "react-router";

import { $api } from "@/api/client";
import { ApiError, errorMessage } from "@/api/errors";
import { paths } from "@/app/paths";
import { ErrorState } from "@/components/common/states";
import { IdeaDetailView } from "@/components/ideas/idea-detail";
import { IdeaParticipation } from "@/components/ideas/idea-participation";
import { Container } from "@/components/layout/container";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { useCanonicalLink, usePageDescription } from "@/hooks/use-page-meta";

/** Slugs are lowercase words separated by hyphens (backend `is_valid_slug`). */
const SLUG_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

function BackLink() {
  return (
    <Link
      to={paths.home}
      className="inline-flex items-center gap-2 rounded-sm text-sm font-medium text-muted-foreground hover:text-foreground"
    >
      <ArrowLeftIcon className="size-4" aria-hidden="true" />
      Retour au classement
    </Link>
  );
}

/**
 * Page of a published idea (GET /api/ideas/{slug}). A merged duplicate answers with a redirect
 * to the idea it joined: fetch follows it, and the address is replaced by the parent's slug.
 * Unknown, hidden and purged ideas show a 404 page.
 */
export function IdeaPage() {
  const { slug = "" } = useParams();
  const navigate = useNavigate();
  const validSlug = SLUG_PATTERN.test(slug) && slug.length <= 80;
  const query = $api.useQuery(
    "get",
    "/api/ideas/{slug}",
    { params: { path: { slug } } },
    { enabled: validSlug },
  );
  const idea = query.data;

  useDocumentTitle(idea?.title ?? (query.isError || !validSlug ? "Idée introuvable" : "Idée"));
  usePageDescription(idea?.summary);
  useCanonicalLink(idea?.canonical_url);

  useEffect(() => {
    if (idea && idea.slug !== slug) {
      void navigate(paths.idea(idea.slug), { replace: true });
    }
  }, [idea, slug, navigate]);

  const notFound = !validSlug || (query.error instanceof ApiError && query.error.status === 404);

  return (
    <Container className="space-y-6 py-8 sm:py-10">
      <BackLink />
      {notFound ? (
        <IdeaNotFound />
      ) : query.isError ? (
        <>
          <h1 className="sr-only">Idée « {slug} »</h1>
          <ErrorState
            title="Cette idée n’a pas pu être chargée"
            message={errorMessage(query.error)}
            onRetry={() => {
              void query.refetch();
            }}
          />
        </>
      ) : idea ? (
        <IdeaDetailView idea={idea} actions={<IdeaParticipation slug={idea.slug} />} />
      ) : (
        <>
          <h1 className="sr-only">Idée « {slug} »</h1>
          <IdeaSkeleton />
        </>
      )}
    </Container>
  );
}

function IdeaNotFound() {
  return (
    <div className="flex flex-col items-center gap-5 py-12 text-center">
      <meta name="robots" content="noindex" />
      <SearchXIcon className="size-12 text-muted-foreground" aria-hidden="true" />
      <div className="space-y-3">
        <p className="text-sm font-semibold tracking-wide text-primary uppercase">Erreur 404</p>
        <h1 className="text-3xl font-semibold">Idée introuvable</h1>
        <p className="max-w-md text-pretty text-muted-foreground">
          Cette idée n’existe pas ou n’est plus publiée : elle a pu être retirée par son auteur ou
          masquée après examen. Vérifiez l’adresse, ou parcourez le classement.
        </p>
      </div>
      <div className="flex flex-wrap justify-center gap-3">
        <Button asChild>
          <Link to={paths.home}>Voir le classement</Link>
        </Button>
        <Button asChild variant="outline">
          <Link to={paths.propose}>Proposer une idée</Link>
        </Button>
      </div>
    </div>
  );
}

function IdeaSkeleton() {
  return (
    <div role="status" aria-live="polite" className="space-y-6">
      <span className="sr-only">Chargement de l’idée…</span>
      <div className="flex gap-2" aria-hidden="true">
        <Skeleton className="h-6 w-24 rounded-full" />
        <Skeleton className="h-6 w-32 rounded-full" />
      </div>
      <Skeleton className="h-10 w-3/4" aria-hidden="true" />
      <div className="space-y-2" aria-hidden="true">
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-5/6" />
      </div>
      <div className="grid gap-4 sm:grid-cols-2" aria-hidden="true">
        <Skeleton className="h-36 rounded-lg" />
        <Skeleton className="h-36 rounded-lg" />
      </div>
    </div>
  );
}
