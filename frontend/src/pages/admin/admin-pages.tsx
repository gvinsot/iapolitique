import { useQueryClient } from "@tanstack/react-query";
import { DownloadIcon, Loader2Icon, RefreshCwIcon, SearchIcon } from "lucide-react";
import { type SubmitEvent, useId, useState } from "react";
import { Link, Navigate, useParams } from "react-router";
import { toast } from "sonner";

import { $api, apiClient } from "@/api/client";
import { errorMessage, requireData } from "@/api/errors";
import type {
  AdminReplyContent,
  AdminScreeningLabelRequest,
  AdminThresholdsUpdate,
  EscalationOutcome,
  ReviewResolution,
} from "@/api/types";
import { paths } from "@/app/paths";
import { ActionsTable, CostsTable, JobsTable, RepliesList } from "@/components/admin/admin-lists";
import { adminLinkClass } from "@/components/admin/admin-styles";
import {
  AdminIdeaLink,
  AdminPageHeader,
  AdminPagination,
  AdminQuery,
  FilterSelect,
} from "@/components/admin/admin-ui";
import {
  DashboardQueues,
  DashboardTiles,
  ScreeningRatesTable,
} from "@/components/admin/dashboard-tiles";
import {
  EscalationsTable,
  IdeasTable,
  ReportsTable,
  ReviewRequestsTable,
} from "@/components/admin/moderation-tables";
import { RightOfReplyEditor } from "@/components/admin/right-of-reply-editor";
import { ScreeningDecisions } from "@/components/admin/screening-decisions";
import {
  DedupThresholdForm,
  ThresholdSettingsForm,
} from "@/components/admin/threshold-settings-form";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAdminListParams } from "@/hooks/use-admin-list-params";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";
import {
  ideaStatusLabels,
  jobKindLabels,
  jobStatusLabels,
  parisToday,
  reportStatusLabels,
  screeningOutcomeLabels,
  targetKindLabels,
} from "@/lib/admin-model";
import { reportReasonLabels } from "@/lib/labels";
import { labelledOptions } from "@/lib/taxonomy-labels";

/** Admin lists show 25 rows per page (the API accepts 50 at most). */
const PER_PAGE = 25;

/** A value of the URL when it belongs to the closed list, else undefined (no filter). */
function pick<K extends string>(
  value: string,
  allowed: Readonly<Record<K, string>>,
): K | undefined {
  return Object.hasOwn(allowed, value) ? (value as K) : undefined;
}

function entries(labels: Readonly<Record<string, string>>): [string, string][] {
  return Object.entries(labels);
}

const pendingDoneLabels = { pending: "En attente", done: "Traitées" } as const;

/** Invalidates every cached GET of these admin paths after a change. */
function useInvalidate() {
  const queryClient = useQueryClient();
  return (...pathsToRefresh: string[]) =>
    Promise.all(
      pathsToRefresh.map((path) => queryClient.invalidateQueries({ queryKey: ["get", path] })),
    );
}

// --- Dashboard and costs ---------------------------------------------------------------------

function useDashboard() {
  return $api.useQuery("get", "/api/admin/dashboard", {}, { refetchInterval: 60_000 });
}

export function AdminDashboardPage() {
  const query = useDashboard();
  return (
    <div className="space-y-8">
      <AdminPageHeader
        title="Tableau de bord"
        description="Vue d’ensemble de l’activité : file de traitement, filtrage, dépenses IA et files d’attente."
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              void query.refetch();
            }}
          >
            <RefreshCwIcon aria-hidden="true" />
            Actualiser
          </Button>
        }
      />
      <AdminQuery query={query} label="le tableau de bord">
        {(dashboard) => (
          <div className="space-y-8">
            <DashboardTiles dashboard={dashboard} />
            <section aria-labelledby="files-attente" className="space-y-3">
              <h2 id="files-attente" className="text-lg font-semibold">
                À traiter
              </h2>
              <DashboardQueues queues={dashboard.queues} />
            </section>
            <ScreeningRatesTable dashboard={dashboard} />
          </div>
        )}
      </AdminQuery>
    </div>
  );
}

export function AdminCostsPage() {
  const query = useDashboard();
  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Coûts IA"
        description="Appels IA et coûts estimés du jour et du mois (Europe/Paris), et plafond journalier de Mistral."
      />
      <AdminQuery query={query} label="les coûts IA">
        {(dashboard) => (
          <div className="space-y-6">
            <DashboardTiles dashboard={dashboard} />
            <CostsTable costs={dashboard.ai_costs} />
            <p className="text-sm text-muted-foreground">
              Coûts estimés à partir des prix unitaires configurés : ils peuvent différer de la
              facture de Mistral AI. Une fois le plafond atteint, les jobs IA attendent minuit.
            </p>
          </div>
        )}
      </AdminQuery>
    </div>
  );
}

// --- Jobs ------------------------------------------------------------------------------------

export function AdminJobsPage() {
  const list = useAdminListParams(["status", "kind"] as const);
  const invalidate = useInvalidate();
  const query = $api.useQuery("get", "/api/admin/jobs", {
    params: {
      query: {
        status: pick(list.values.status, jobStatusLabels),
        kind: pick(list.values.kind, jobKindLabels),
        page: list.page,
        per_page: PER_PAGE,
      },
    },
  });

  const retry = async (id: number) => {
    await apiClient.POST("/api/admin/jobs/{id}/retry", { params: { path: { id } } });
    toast.success(`Job n° ${id} remis en file.`);
    await invalidate("/api/admin/jobs", "/api/admin/dashboard");
  };

  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Jobs"
        description="File de traitement du worker, plus récents d’abord. Un job abandonné peut être relancé."
      />
      <div className="flex flex-wrap gap-4">
        <FilterSelect
          label="Statut"
          all="Tous les statuts"
          value={list.values.status}
          onChange={(value) => {
            list.set("status", value);
          }}
          options={entries(jobStatusLabels)}
        />
        <FilterSelect
          label="Type"
          all="Tous les types"
          value={list.values.kind}
          onChange={(value) => {
            list.set("kind", value);
          }}
          options={entries(jobKindLabels)}
        />
      </div>
      <AdminQuery query={query} label="les jobs">
        {(page) => (
          <>
            <JobsTable rows={page.items} onRetry={retry} />
            <AdminPagination
              page={page.page}
              totalPages={page.total_pages}
              hrefForPage={list.hrefForPage}
            />
          </>
        )}
      </AdminQuery>
    </div>
  );
}

// --- Ideas and rights of reply ---------------------------------------------------------------

function SearchForm({
  value,
  onSearch,
  label,
  placeholder,
}: {
  value: string;
  onSearch: (value: string) => void;
  label: string;
  placeholder: string;
}) {
  const id = useId();
  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const text = new FormData(event.currentTarget).get("q");
    onSearch(typeof text === "string" ? text.trim() : "");
  };
  return (
    <form role="search" onSubmit={submit} className="flex items-end gap-2" key={value}>
      <div className="space-y-1">
        <Label htmlFor={id} className="text-xs text-muted-foreground">
          {label}
        </Label>
        <Input
          id={id}
          name="q"
          type="search"
          defaultValue={value}
          maxLength={200}
          placeholder={placeholder}
          className="w-72 max-w-full"
        />
      </div>
      <Button type="submit" variant="secondary">
        <SearchIcon aria-hidden="true" />
        Rechercher
      </Button>
    </form>
  );
}

export function AdminIdeasPage() {
  const list = useAdminListParams(["status", "q"] as const);
  const invalidate = useInvalidate();
  const query = $api.useQuery("get", "/api/admin/ideas", {
    params: {
      query: {
        status: pick(list.values.status, ideaStatusLabels),
        q: list.values.q || undefined,
        page: list.page,
        per_page: PER_PAGE,
      },
    },
  });

  const refresh = () => invalidate("/api/admin/ideas", "/api/admin/dashboard");
  const hide = async (id: string, reason: string) => {
    await apiClient.POST("/api/admin/ideas/{id}/hide", {
      params: { path: { id } },
      body: { reason },
    });
    toast.success("Idée masquée. Les exports seront régénérés.");
    await refresh();
  };
  const republish = async (id: string, reason: string) => {
    await apiClient.POST("/api/admin/ideas/{id}/republish", {
      params: { path: { id } },
      body: { reason },
    });
    toast.success("Idée republiée. Les exports seront régénérés.");
    await refresh();
  };
  const reanalyze = async (id: string) => {
    const { data } = await apiClient.POST("/api/admin/ideas/{id}/reanalyze", {
      params: { path: { id } },
    });
    toast.success(
      requireData(data).queued
        ? "Analyse mise en file."
        : "Une analyse de cette idée était déjà en file : elle s’exécutera.",
    );
    await refresh();
  };

  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Idées"
        description="Idées de tout statut, plus récemment modifiées d’abord : masquer, republier, relancer une analyse, publier un droit de réponse."
      />
      <div className="flex flex-wrap items-end gap-4">
        <FilterSelect
          label="Statut"
          all="Tous les statuts"
          value={list.values.status}
          onChange={(value) => {
            list.set("status", value);
          }}
          options={entries(ideaStatusLabels)}
        />
        <SearchForm
          label="Recherche"
          placeholder="Identifiant, titre, slug ou texte"
          value={list.values.q}
          onSearch={(value) => {
            list.set("q", value);
          }}
        />
      </div>
      <AdminQuery query={query} label="les idées">
        {(page) => (
          <>
            <IdeasTable
              rows={page.items}
              onHide={hide}
              onRepublish={republish}
              onReanalyze={reanalyze}
            />
            <AdminPagination
              page={page.page}
              totalPages={page.total_pages}
              hrefForPage={list.hrefForPage}
            />
          </>
        )}
      </AdminQuery>
    </div>
  );
}

export function AdminRepliesPage() {
  const { id = "" } = useParams();
  const invalidate = useInvalidate();
  const ideaQuery = $api.useQuery("get", "/api/admin/ideas", {
    params: { query: { q: id, per_page: 1 } },
  });
  const repliesQuery = $api.useQuery("get", "/api/admin/ideas/{id}/replies", {
    params: { path: { id } },
  });
  const idea = ideaQuery.data?.items.find((item) => item.id === id);
  const title = idea?.title ?? "Idée";

  const refresh = () => invalidate("/api/admin/ideas/{id}/replies", "/api/admin/ideas");
  const create = async (content: AdminReplyContent) => {
    await apiClient.POST("/api/admin/ideas/{id}/replies", {
      params: { path: { id } },
      body: content,
    });
    await refresh();
  };
  const update = async (replyId: string, content: AdminReplyContent) => {
    await apiClient.PUT("/api/admin/replies/{id}", {
      params: { path: { id: replyId } },
      body: content,
    });
    await refresh();
  };
  const remove = async (replyId: string) => {
    await apiClient.DELETE("/api/admin/replies/{id}", { params: { path: { id: replyId } } });
    toast.success("Droit de réponse retiré.");
    await refresh();
  };

  return (
    <div className="space-y-8">
      <AdminPageHeader
        title="Droits de réponse"
        description={
          <>
            Réponses publiées sous l’idée (LCEN art. 1-1 III), texte reproduit sans modification,
            dans les 3 jours suivant la demande.{" "}
            <Link to={paths.admin.ideas} className={adminLinkClass}>
              Retour aux idées
            </Link>
          </>
        }
      />
      {idea ? (
        <p>
          <AdminIdeaLink idea={idea} />
        </p>
      ) : null}
      <section aria-labelledby="reponses-publiees" className="space-y-3">
        <h2 id="reponses-publiees" className="text-lg font-semibold">
          Réponses publiées
        </h2>
        <AdminQuery query={repliesQuery} label="les droits de réponse">
          {(replies) => (
            <RepliesList ideaTitle={title} replies={replies} onUpdate={update} onDelete={remove} />
          )}
        </AdminQuery>
      </section>
      <section aria-labelledby="nouvelle-reponse" className="space-y-3">
        <h2 id="nouvelle-reponse" className="text-lg font-semibold">
          Publier une nouvelle réponse
        </h2>
        {idea && idea.status !== "published" && idea.status !== "hidden" ? (
          <p className="text-muted-foreground">
            {idea.status === "withdrawn"
              ? "Cette idée a été retirée par son auteur : elle n’est plus publiée et aucun droit de réponse ne peut y être ajouté."
              : "Un droit de réponse ne peut être publié que sous une idée publiée ou masquée."}
          </p>
        ) : (
          <RightOfReplyEditor ideaTitle={title} onSubmit={create} resetOnSuccess />
        )}
      </section>
    </div>
  );
}

// --- Reports, escalations, review requests ---------------------------------------------------

export function AdminReportsPage() {
  const list = useAdminListParams(["status", "reason", "idea_id"] as const);
  const labels = useTaxonomyLabels();
  const query = $api.useQuery("get", "/api/admin/reports", {
    params: {
      query: {
        status: pick(list.values.status, reportStatusLabels),
        reason: pick(list.values.reason, reportReasonLabels),
        idea_id: list.values.idea_id || undefined,
        page: list.page,
        per_page: PER_PAGE,
      },
    },
  });

  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Signalements"
        description="Signalements reçus, plus récents d’abord, avec la revue par IA. Le nom et l’e-mail facultatifs du signalant ne sont jamais publiés et sont effacés après 6 mois."
      />
      <div className="flex flex-wrap gap-4">
        <FilterSelect
          label="Statut"
          all="Tous les statuts"
          value={list.values.status}
          onChange={(value) => {
            list.set("status", value);
          }}
          options={entries(reportStatusLabels)}
        />
        <FilterSelect
          label="Motif"
          all="Tous les motifs"
          value={list.values.reason}
          onChange={(value) => {
            list.set("reason", value);
          }}
          options={labelledOptions(reportReasonLabels, labels.reportReasons)}
        />
      </div>
      {list.values.idea_id ? (
        <p className="text-sm">
          Signalements d’une seule idée.{" "}
          <button
            type="button"
            className={adminLinkClass}
            onClick={() => {
              list.set("idea_id", "");
            }}
          >
            Voir tous les signalements
          </button>
        </p>
      ) : null}
      <AdminQuery query={query} label="les signalements">
        {(page) => (
          <>
            <ReportsTable rows={page.items} />
            <AdminPagination
              page={page.page}
              totalPages={page.total_pages}
              hrefForPage={list.hrefForPage}
            />
          </>
        )}
      </AdminQuery>
    </div>
  );
}

export function AdminEscalationsPage() {
  const list = useAdminListParams(["status"] as const);
  const invalidate = useInvalidate();
  const query = $api.useQuery("get", "/api/admin/escalations", {
    params: {
      query: {
        status: pick(list.values.status, pendingDoneLabels),
        page: list.page,
        per_page: PER_PAGE,
      },
    },
  });

  const handle = async (id: string, outcome: EscalationOutcome, note: string | null) => {
    await apiClient.POST("/api/admin/escalations/{id}/handle", {
      params: { path: { id } },
      body: { outcome, note },
    });
    toast.success("Escalade traitée.");
    await invalidate("/api/admin/escalations", "/api/admin/dashboard", "/api/admin/ideas");
  };

  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Escalades"
        description="Contenus graves masqués automatiquement après la revue d’un signalement (AGENTS.md §7.5) : à examiner et, si besoin, à signaler à PHAROS sous 24 h."
      />
      <FilterSelect
        label="Statut"
        all="Toutes (en attente d’abord)"
        value={list.values.status}
        onChange={(value) => {
          list.set("status", value);
        }}
        options={entries(pendingDoneLabels)}
      />
      <AdminQuery query={query} label="les escalades">
        {(page) => (
          <>
            <EscalationsTable rows={page.items} onHandle={handle} />
            <AdminPagination
              page={page.page}
              totalPages={page.total_pages}
              hrefForPage={list.hrefForPage}
            />
          </>
        )}
      </AdminQuery>
    </div>
  );
}

export function AdminReviewRequestsPage() {
  const list = useAdminListParams(["status"] as const);
  const invalidate = useInvalidate();
  const query = $api.useQuery("get", "/api/admin/review-requests", {
    params: {
      query: {
        status: pick(list.values.status, pendingDoneLabels),
        page: list.page,
        per_page: PER_PAGE,
      },
    },
  });

  const resolve = async (id: string, resolution: ReviewResolution, note: string | null) => {
    await apiClient.POST("/api/admin/review-requests/{id}/resolve", {
      params: { path: { id } },
      body: { resolution, note },
    });
    toast.success(resolution === "overturned" ? "Décision infirmée." : "Décision confirmée.");
    await invalidate(
      "/api/admin/review-requests",
      "/api/admin/dashboard",
      "/api/admin/ideas",
      "/api/admin/screenings",
    );
  };

  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Réexamens"
        description="Demandes de réexamen humain d’une décision automatique (refus ou masquage), envoyées par les auteurs depuis leur lien de suivi."
      />
      <FilterSelect
        label="Statut"
        all="Toutes (en attente d’abord)"
        value={list.values.status}
        onChange={(value) => {
          list.set("status", value);
        }}
        options={entries(pendingDoneLabels)}
      />
      <AdminQuery query={query} label="les demandes de réexamen">
        {(page) => (
          <>
            <ReviewRequestsTable rows={page.items} onResolve={resolve} />
            <AdminPagination
              page={page.page}
              totalPages={page.total_pages}
              hrefForPage={list.hrefForPage}
            />
          </>
        )}
      </AdminQuery>
    </div>
  );
}

// --- Screening decisions and calibration export ----------------------------------------------

const labeledLabels = { true: "Étiquetées", false: "Décidées, non étiquetées" } as const;
const exportParts = {
  results: "Réponses brutes (screening_results.jsonl)",
  samples: "Exemples étiquetés (screening_samples.jsonl)",
} as const;

function CalibrationExport() {
  const id = useId();
  const [part, setPart] = useState<keyof typeof exportParts>("results");
  const [kind, setKind] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const download = async () => {
    setPending(true);
    setError(null);
    try {
      // The admin token travels in a header: the file is fetched, then saved from memory.
      const { data } = await apiClient.GET("/api/admin/screenings/export.jsonl", {
        params: { query: { part, kind: pick(kind, targetKindLabels) } },
        parseAs: "blob",
      });
      const url = URL.createObjectURL(requireData(data));
      const link = document.createElement("a");
      link.href = url;
      link.download = `iapolitique-screening-${part}-${parisToday()}.jsonl`;
      document.body.append(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  return (
    <section aria-labelledby={`${id}-title`} className="space-y-3 rounded-lg border bg-card p-4">
      <h2 id={`${id}-title`} className="text-lg font-semibold">
        Export pour le banc de calibration
      </h2>
      <p className="text-sm text-muted-foreground">
        Fichiers JSON Lines au format de <code>docs/calibration/bench_screening.py</code>. Les
        textes purgés sont exportés vides : utilisables avec <code>eval</code> et <code>sweep</code>
        , pas avec <code>run</code>.
      </p>
      <div className="flex flex-wrap items-end gap-4">
        <FilterSelect
          label="Fichier"
          all="Réponses brutes (par défaut)"
          value={part === "results" ? "" : part}
          onChange={(value) => {
            setPart(value === "samples" ? "samples" : "results");
          }}
          options={[["samples", exportParts.samples]]}
        />
        <FilterSelect
          label="Textes"
          all="Tous les types"
          value={kind}
          onChange={setKind}
          options={entries(targetKindLabels)}
        />
        <Button
          type="button"
          variant="outline"
          disabled={pending}
          onClick={() => {
            void download();
          }}
        >
          {pending ? (
            <Loader2Icon className="animate-spin" aria-hidden="true" />
          ) : (
            <DownloadIcon aria-hidden="true" />
          )}
          Télécharger
        </Button>
      </div>
      {error ? (
        <p role="alert" className="text-sm font-medium text-destructive">
          {error}
        </p>
      ) : null}
    </section>
  );
}

export function AdminDecisionsPage() {
  const list = useAdminListParams(["kind", "outcome", "labeled", "reason_code"] as const);
  const invalidate = useInvalidate();
  const labeled = pick(list.values.labeled, labeledLabels);
  const query = $api.useQuery("get", "/api/admin/screenings", {
    params: {
      query: {
        kind: pick(list.values.kind, targetKindLabels),
        outcome: pick(list.values.outcome, screeningOutcomeLabels),
        labeled: labeled === undefined ? undefined : labeled === "true",
        reason_code: list.values.reason_code || undefined,
        page: list.page,
        per_page: PER_PAGE,
      },
    },
  });

  const refresh = () => invalidate("/api/admin/screenings", "/api/admin/dashboard");
  const label = async (id: string, request: AdminScreeningLabelRequest) => {
    await apiClient.PUT("/api/admin/screenings/{id}/label", {
      params: { path: { id } },
      body: request,
    });
    await refresh();
  };
  const unlabel = async (id: string) => {
    await apiClient.DELETE("/api/admin/screenings/{id}/label", { params: { path: { id } } });
    await refresh();
  };

  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Décisions de filtrage"
        description="Probabilités brutes du modèle de décision par critère, seuils appliqués et étiquetage des décisions pour calibrer les seuils."
      />
      <div className="flex flex-wrap gap-4">
        <FilterSelect
          label="Textes"
          all="Tous les types"
          value={list.values.kind}
          onChange={(value) => {
            list.set("kind", value);
          }}
          options={entries(targetKindLabels)}
        />
        <FilterSelect
          label="Issue"
          all="Toutes les issues"
          value={list.values.outcome}
          onChange={(value) => {
            list.set("outcome", value);
          }}
          options={entries(screeningOutcomeLabels)}
        />
        <FilterSelect
          label="Étiquetage"
          all="Toutes les décisions"
          value={list.values.labeled}
          onChange={(value) => {
            list.set("labeled", value);
          }}
          options={entries(labeledLabels)}
        />
      </div>
      <AdminQuery query={query} label="les décisions de filtrage">
        {(page) => (
          <>
            <p className="text-sm text-muted-foreground" role="status">
              {page.total} décision{page.total > 1 ? "s" : ""}
            </p>
            <ScreeningDecisions screenings={page.items} onLabel={label} onUnlabel={unlabel} />
            <AdminPagination
              page={page.page}
              totalPages={page.total_pages}
              hrefForPage={list.hrefForPage}
            />
          </>
        )}
      </AdminQuery>
      <CalibrationExport />
    </div>
  );
}

// --- Settings --------------------------------------------------------------------------------

export function AdminSettingsPage() {
  const query = $api.useQuery("get", "/api/admin/settings", {});
  const invalidate = useInvalidate();

  const saveThresholds = async (update: AdminThresholdsUpdate) => {
    await apiClient.PUT("/api/admin/settings/screening-thresholds", { body: update });
    await invalidate("/api/admin/settings", "/api/admin/screenings");
  };
  const saveDedup = async (value: number | null) => {
    await apiClient.PUT("/api/admin/settings/dedup-threshold", { body: { value } });
    await invalidate("/api/admin/settings");
  };

  return (
    <div className="space-y-8">
      <AdminPageHeader
        title="Réglages"
        description="Seuils de filtrage par critère et seuil de détection des doublons. Toute modification est consignée au journal."
      />
      <AdminQuery query={query} label="les réglages">
        {(settings) => (
          <div className="space-y-10">
            <section aria-labelledby="seuils-filtrage" className="space-y-4">
              <h2 id="seuils-filtrage" className="text-xl font-semibold">
                Filtrage
              </h2>
              <ThresholdSettingsForm settings={settings} onSave={saveThresholds} />
            </section>
            <section aria-labelledby="seuil-doublon" className="space-y-4">
              <h2 id="seuil-doublon" className="text-xl font-semibold">
                Doublons
              </h2>
              <DedupThresholdForm settings={settings} onSave={saveDedup} />
            </section>
          </div>
        )}
      </AdminQuery>
    </div>
  );
}

// --- Journal ---------------------------------------------------------------------------------

export function AdminJournalPage() {
  const list = useAdminListParams(["action", "target_id"] as const);
  const query = $api.useQuery("get", "/api/admin/actions", {
    params: {
      query: {
        action: list.values.action || undefined,
        target_id: list.values.target_id || undefined,
        page: list.page,
        per_page: PER_PAGE,
      },
    },
  });

  return (
    <div className="space-y-6">
      <AdminPageHeader
        title="Journal"
        description="Toutes les actions d’administration, plus récentes d’abord, avec leur cible et leurs détails (sans texte de visiteur)."
      />
      <div className="flex flex-wrap gap-4">
        <SearchForm
          label="Action exacte"
          placeholder="Ex. : idea.hide"
          value={list.values.action}
          onSearch={(value) => {
            list.set("action", value);
          }}
        />
        <SearchForm
          label="Identifiant de la cible"
          placeholder="Identifiant d’idée, de job…"
          value={list.values.target_id}
          onSearch={(value) => {
            list.set("target_id", value);
          }}
        />
      </div>
      <AdminQuery query={query} label="le journal">
        {(page) => (
          <>
            <ActionsTable rows={page.items} />
            <AdminPagination
              page={page.page}
              totalPages={page.total_pages}
              hrefForPage={list.hrefForPage}
            />
          </>
        )}
      </AdminQuery>
    </div>
  );
}

/** Former address of the moderation page. */
export function AdminModerationRedirect() {
  return <Navigate to={paths.admin.reports} replace />;
}
