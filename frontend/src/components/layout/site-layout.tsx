import type { ReactNode } from "react";
import { Outlet, ScrollRestoration } from "react-router";

import { RouteFocus } from "./route-focus";
import { SiteFooter } from "./site-footer";
import { SiteHeader } from "./site-header";
import { MAIN_CONTENT_ID, SkipLink } from "./skip-link";

/** Public site shell: skip link, header, main landmark, footer. */
export function SiteLayout({ children }: { children?: ReactNode }) {
  return (
    <div className="flex min-h-dvh flex-col">
      <SkipLink />
      <SiteHeader />
      <main id={MAIN_CONTENT_ID} tabIndex={-1} className="flex-1 outline-none">
        {children ?? <Outlet />}
      </main>
      <SiteFooter />
      <ScrollRestoration />
      <RouteFocus />
    </div>
  );
}
