import { useEffect } from "react";

import { SITE_NAME, SITE_TAGLINE } from "@/app/paths";

export function formatDocumentTitle(title?: string): string {
  return title ? `${title} · ${SITE_NAME}` : `${SITE_NAME} · ${SITE_TAGLINE}`;
}

/** Sets the browser tab title (also announced by screen readers after a navigation). */
export function useDocumentTitle(title?: string): void {
  useEffect(() => {
    document.title = formatDocumentTitle(title);
  }, [title]);
}
