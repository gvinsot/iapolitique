import { HistoryIcon, KeyRoundIcon, LinkIcon } from "lucide-react";
import { Link, Navigate, useLocation, useParams } from "react-router";

import { ApiError, errorMessage } from "@/api/errors";
import { paths } from "@/app/paths";
import { PageHeader } from "@/components/common/page-header";
import { ErrorState, ListSkeleton } from "@/components/common/states";
import { Container } from "@/components/layout/container";
import { RememberLinkToggle } from "@/components/tracking/remember-link-toggle";
import { TrackingActions } from "@/components/tracking/tracking-actions";
import { TrackingStatus } from "@/components/tracking/tracking-status";
import { RememberedLinks, TrackingTokenEntry } from "@/components/tracking/tracking-token-entry";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { useDeleteTrackedItem, useRequestHumanReview, useTrackedItem } from "@/hooks/use-tracking";
import { formatDate } from "@/lib/format";
import { trackedKindLabels, type TrackedItem } from "@/lib/tracking-model";
import { TRACKING_PATH, tokenFromHash } from "@/lib/tracking-token";

/** Navigation state set by LegacyTrackRedirect. */
interface TrackLocationState {
  legacyLink?: boolean;
}

function isTrackLocationState(value: unknown): value is TrackLocationState {
  return typeof value === "object" && value !== null && "legacyLink" in value;
}

/**
 * Anonymous follow-up of a submission (AGENTS.md §7.1). The token is read from the URL
 * fragment, which browsers never send to a server, and posted to the API in a request body.
 */
export function TrackPage() {
  useDocumentTitle("Suivi de votre proposition");
  const location = useLocation();
  const hash = location.hash;
  const token = tokenFromHash(hash);
  const legacy = isTrackLocationState(location.state) && location.state.legacyLink === true;

  return (
    <>
      {/* Private page: out of search engines, and never leak its address to another site. */}
      <meta name="robots" content="noindex, nofollow" />
      <meta name="referrer" content="no-referrer" />
      <PageHeader
        eyebrow="Lien privé"
        title="Suivi de votre proposition"
        description="Où en est votre idée, votre contribution ou votre signalement, et ce que vous pouvez faire."
      />
      <Container className="grid gap-8 py-10 lg:grid-cols-[2fr_1fr]">
        <div className="min-w-0 space-y-6">
          {legacy ? <LegacyLinkNotice /> : null}
          {token ? (
            <TrackedSubmission token={token} />
          ) : (
            <>
              {hash.length > 1 ? (
                <Alert variant="destructive">
                  <LinkIcon aria-hidden="true" />
                  <AlertTitle>Ce lien de suivi n’est pas valide</AlertTitle>
                  <AlertDescription>
                    <p>
                      Vérifiez qu’il a été copié en entier : un lien de suivi se termine par une
                      suite de 43 caractères (lettres, chiffres, « - » et « _ »).
                    </p>
                  </AlertDescription>
                </Alert>
              ) : (
                <p className="text-muted-foreground">
                  Collez le lien reçu après votre envoi pour en afficher le suivi.
                </p>
              )}
              <TrackingTokenEntry />
              <RememberedLinks />
            </>
          )}
        </div>

        <aside aria-label="Confidentialité du lien">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <KeyRoundIcon className="size-4" aria-hidden="true" />
                <h2>Un lien personnel et anonyme</h2>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-sm text-muted-foreground">
              <p>
                Ce lien n’est enregistré nulle part en clair : seule une empreinte chiffrée est
                conservée. Il ne permet pas de vous identifier, et la partie secrète, après le « #
                », n’est jamais transmise dans l’adresse des pages.
              </p>
              <p>
                Toute personne qui le possède peut consulter ce suivi, supprimer le texte ou
                demander un réexamen : ne le partagez pas. En cas de perte, il ne peut pas être
                retrouvé.
              </p>
            </CardContent>
          </Card>
        </aside>
      </Container>
    </>
  );
}

function LegacyLinkNotice() {
  return (
    <Alert variant="warning">
      <HistoryIcon aria-hidden="true" />
      <AlertTitle>Lien à l’ancien format</AlertTitle>
      <AlertDescription>
        <p>
          Vous avez ouvert un lien de la forme « /suivi/… », où la partie secrète figure dans
          l’adresse de la page : elle a pu être enregistrée dans les journaux techniques du site,
          conservés 7 jours au plus. Utilisez désormais l’adresse affichée dans votre navigateur («
          /suivi#… ») et, si vous avez mémorisé ou noté l’ancien lien, remplacez-le.
        </p>
      </AlertDescription>
    </Alert>
  );
}

function TrackedSubmission({ token }: { token: string }) {
  const query = useTrackedItem(token);
  const deletion = useDeleteTrackedItem(token);
  const review = useRequestHumanReview(token);

  if (query.isPending) return <ListSkeleton rows={2} label="Chargement du suivi…" />;
  if (query.isError) {
    const notFound = query.error instanceof ApiError && query.error.status === 404;
    return notFound ? (
      <Alert variant="destructive">
        <LinkIcon aria-hidden="true" />
        <AlertTitle>Aucun envoi ne correspond à ce lien</AlertTitle>
        <AlertDescription>
          <p>
            Il a peut-être été mal copié, ou le texte a été supprimé : à votre demande, ou 30 jours
            après un refus. Un lien perdu ne peut pas être retrouvé.
          </p>
        </AlertDescription>
      </Alert>
    ) : (
      <ErrorState
        title="Suivi momentanément indisponible"
        message={errorMessage(query.error)}
        onRetry={() => {
          void query.refetch();
        }}
      />
    );
  }

  const item: TrackedItem = query.data;
  return (
    <>
      <TrackingStatus item={item} />
      <TrackingActions
        item={item}
        onDelete={() => deletion.mutateAsync()}
        onRequestReview={(message) => review.mutateAsync(message)}
      />
      {item.status === "deleted" ? null : (
        <RememberLinkToggle
          token={token}
          label={`${trackedKindLabels[item.kind]} du ${formatDate(item.submitted_on)}`}
        />
      )}
      <Button asChild variant="outline">
        <Link to={paths.home}>Voir le classement des idées</Link>
      </Button>
    </>
  );
}

/**
 * Former links put the token in the path (`/suivi/<token>`): move it to the fragment on the
 * client and warn that it may already have been logged by the server.
 */
export function LegacyTrackRedirect() {
  const { token = "" } = useParams();
  return (
    <Navigate
      to={{ pathname: TRACKING_PATH, hash: token }}
      replace
      state={{ legacyLink: true } satisfies TrackLocationState}
    />
  );
}
