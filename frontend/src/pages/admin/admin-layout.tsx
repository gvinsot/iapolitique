import { ExternalLinkIcon, LockKeyholeIcon, LogOutIcon } from "lucide-react";
import { Link, NavLink, Outlet, ScrollRestoration } from "react-router";

import { errorMessage } from "@/api/errors";
import { paths, SITE_NAME } from "@/app/paths";
import { AdminSignIn } from "@/components/admin/admin-sign-in";
import { PageLoading } from "@/components/common/page-loading";
import { ErrorState } from "@/components/common/states";
import { BrandMark } from "@/components/layout/brand-mark";
import { RouteFocus } from "@/components/layout/route-focus";
import { MAIN_CONTENT_ID, SkipLink } from "@/components/layout/skip-link";
import { ThemeToggle } from "@/components/theme/theme-toggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { type AdminSessionState, useAdminSession } from "@/hooks/use-admin-session";
import { cn } from "@/lib/utils";

const adminNavItems = [
  { to: paths.admin.dashboard, label: "Tableau de bord", end: true },
  { to: paths.admin.ideas, label: "Idées" },
  { to: paths.admin.reports, label: "Signalements" },
  { to: paths.admin.escalations, label: "Escalades" },
  { to: paths.admin.seals, label: "Copies scellées" },
  { to: paths.admin.reviewRequests, label: "Réexamens" },
  { to: paths.admin.decisions, label: "Décisions de filtrage" },
  { to: paths.admin.jobs, label: "Jobs" },
  { to: paths.admin.costs, label: "Coûts IA" },
  { to: paths.admin.settings, label: "Réglages" },
  { to: paths.admin.journal, label: "Journal" },
] as const;

function AdminContent({
  state,
  onSignIn,
}: {
  state: AdminSessionState;
  onSignIn: (token: string) => void;
}) {
  switch (state.status) {
    case "signed-out":
      return <AdminSignIn onSubmit={onSignIn} rejected={state.rejected} />;
    case "checking":
      return <PageLoading />;
    case "wrong-host":
      // Public host (findings ADM-1, FE-5): no form, no retry, the token never goes there.
      return (
        <ErrorState
          title="Administration indisponible à cette adresse"
          message="Administration disponible uniquement sur l’adresse du réseau local. Ouvrez l’administration sur son adresse dédiée ; aucun jeton n’est demandé ni envoyé ici."
        />
      );
    case "error":
      return (
        <ErrorState
          title="Administration injoignable"
          message={`${errorMessage(state.error)} L’administration n’est accessible que depuis le réseau local.`}
          onRetry={state.retry}
        />
      );
    case "authenticated":
      return <Outlet />;
  }
}

/**
 * Back-office shell. Only reachable from the LAN (Traefik allow-list, AGENTS.md §8) and, as
 * defence in depth, behind the admin token checked by the API.
 */
export function AdminLayout() {
  const { state, signIn, signOut, hasToken } = useAdminSession();
  const authenticated = state.status === "authenticated";

  return (
    <div className="flex min-h-dvh flex-col">
      <meta name="robots" content="noindex, nofollow" />
      <SkipLink />
      <header className="border-b bg-card">
        <div className="flex min-h-14 flex-wrap items-center gap-3 px-4 py-2 sm:px-6">
          <Link to={paths.admin.dashboard} className="flex items-center gap-2 rounded-md">
            <BrandMark className="size-7" />
            <span className="font-semibold">{SITE_NAME}</span>
            <span className="text-muted-foreground">· Administration</span>
          </Link>
          <Badge variant="outline" className="gap-1">
            <LockKeyholeIcon aria-hidden="true" />
            Réseau local uniquement
          </Badge>
          <div className="ml-auto flex items-center gap-1">
            <Link
              to={paths.home}
              className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-sm text-muted-foreground hover:text-foreground"
            >
              Site public
              <ExternalLinkIcon className="size-3.5" aria-hidden="true" />
            </Link>
            <ThemeToggle />
            {hasToken ? (
              <Button type="button" variant="ghost" size="sm" onClick={signOut}>
                <LogOutIcon aria-hidden="true" />
                Se déconnecter
              </Button>
            ) : null}
          </div>
        </div>
      </header>

      <div className="flex flex-1 flex-col lg:flex-row">
        {authenticated ? (
          <nav
            aria-label="Navigation de l’administration"
            className="border-b lg:w-60 lg:shrink-0 lg:border-r lg:border-b-0"
          >
            <ul className="flex gap-1 overflow-x-auto p-2 lg:flex-col lg:p-3">
              {adminNavItems.map((item) => (
                <li key={item.to} className="shrink-0">
                  <NavLink
                    to={item.to}
                    end={"end" in item ? item.end : false}
                    className={({ isActive }) =>
                      cn(
                        "block rounded-md px-3 py-2 text-sm font-medium whitespace-nowrap hover:bg-accent hover:text-accent-foreground",
                        isActive ? "bg-secondary text-foreground" : "text-muted-foreground",
                      )
                    }
                  >
                    {item.label}
                  </NavLink>
                </li>
              ))}
            </ul>
          </nav>
        ) : null}

        <main
          id={MAIN_CONTENT_ID}
          tabIndex={-1}
          className="min-w-0 flex-1 px-4 py-6 outline-none sm:px-6 lg:py-8"
        >
          <AdminContent state={state} onSignIn={signIn} />
        </main>
      </div>
      <ScrollRestoration />
      <RouteFocus />
    </div>
  );
}
