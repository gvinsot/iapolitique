import type { RouteObject } from "react-router";

import { paths } from "@/app/paths";
import { PageLoading } from "@/components/common/page-loading";
import { RedirectWithHash } from "@/components/common/redirect-with-hash";
import { SiteLayout } from "@/components/layout/site-layout";
import { AboutPage } from "@/pages/about-page";
import { HomePage } from "@/pages/home-page";
import { IdeaPage } from "@/pages/idea-page";
import { MethodologyPage } from "@/pages/methodology-page";
import { NotFoundPage } from "@/pages/not-found-page";
import { ProposePage } from "@/pages/propose-page";
import { RouteErrorPage } from "@/pages/route-error-page";
import { LegacyTrackRedirect, TrackPage } from "@/pages/track-page";

/**
 * Application routes (French paths). Public pages visited first are bundled eagerly; legal and
 * admin pages are split into their own chunks.
 */
export const routes: RouteObject[] = [
  {
    path: "/",
    Component: SiteLayout,
    ErrorBoundary: RouteErrorPage,
    HydrateFallback: PageLoading,
    children: [
      {
        // Pathless boundary: errors render inside the layout, header and footer stay usable.
        ErrorBoundary: RouteErrorPage,
        children: [
          { index: true, Component: HomePage },
          { path: "proposer", Component: ProposePage },
          { path: "idees/:slug", Component: IdeaPage },
          { path: "suivi", Component: TrackPage },
          { path: "suivi/:token", Component: LegacyTrackRedirect },
          { path: "methodologie", Component: MethodologyPage },
          { path: "a-propos", Component: AboutPage },
          {
            path: "mentions-legales",
            lazy: async () => ({
              Component: (await import("@/pages/legal/legal-notice-page")).LegalNoticePage,
            }),
          },
          {
            path: "confidentialite",
            lazy: async () => ({
              Component: (await import("@/pages/legal/privacy-page")).PrivacyPage,
            }),
          },
          {
            path: "regles",
            lazy: async () => ({
              Component: (await import("@/pages/legal/terms-page")).TermsPage,
            }),
          },
          {
            path: "licence",
            lazy: async () => ({
              Component: (await import("@/pages/legal/licence-page")).LicencePage,
            }),
          },
          // Former address of the rules page: keep old links working (anchors included).
          { path: "cgu", element: <RedirectWithHash to={paths.terms} /> },
          { path: "*", Component: NotFoundPage },
        ],
      },
    ],
  },
  {
    path: "/admin",
    lazy: async () => ({
      Component: (await import("@/pages/admin/admin-layout")).AdminLayout,
    }),
    ErrorBoundary: RouteErrorPage,
    HydrateFallback: PageLoading,
    children: [
      {
        ErrorBoundary: RouteErrorPage,
        children: [
          {
            index: true,
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminDashboardPage,
            }),
          },
          {
            path: "idees",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminIdeasPage,
            }),
          },
          {
            path: "idees/:id/reponses",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminRepliesPage,
            }),
          },
          {
            path: "signalements",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminReportsPage,
            }),
          },
          {
            path: "escalades",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminEscalationsPage,
            }),
          },
          {
            path: "escalades/:id/copie-scellee",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminSealPage,
            }),
          },
          {
            path: "copies-scellees",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminSealsPage,
            }),
          },
          {
            path: "reexamens",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminReviewRequestsPage,
            }),
          },
          {
            path: "decisions",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminDecisionsPage,
            }),
          },
          {
            path: "jobs",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminJobsPage,
            }),
          },
          {
            path: "couts",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminCostsPage,
            }),
          },
          {
            path: "reglages",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminSettingsPage,
            }),
          },
          {
            path: "journal",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminJournalPage,
            }),
          },
          {
            path: "moderation",
            lazy: async () => ({
              Component: (await import("@/pages/admin/admin-pages")).AdminModerationRedirect,
            }),
          },
          { path: "*", Component: NotFoundPage },
        ],
      },
    ],
  },
];
