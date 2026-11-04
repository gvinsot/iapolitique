import { useEffect } from "react";

import { SITE_NAME, SITE_TAGLINE } from "@/app/paths";
import { useOpenGraphTitle } from "@/hooks/use-page-meta";

export function formatDocumentTitle(title?: string): string {
  return title ? `${title} · ${SITE_NAME}` : `${SITE_NAME} · ${SITE_TAGLINE}`;
}

/**
 * Sets the browser tab title (also announced by screen readers after a navigation) and the
 * `og:title` of link previews.
 */
export function useDocumentTitle(title?: string): void {
  const full = formatDocumentTitle(title);
  useEffect(() => {
    document.title = full;
  }, [full]);
  useOpenGraphTitle(full);
}
