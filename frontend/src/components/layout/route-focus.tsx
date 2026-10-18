import { useEffect, useRef } from "react";
import { useLocation } from "react-router";

import { MAIN_CONTENT_ID } from "./skip-link";

/**
 * After a client-side navigation, moves keyboard and screen-reader focus to the main content,
 * as a full page load would. The first render is left alone.
 */
export function RouteFocus() {
  const { pathname } = useLocation();
  const previousPathname = useRef(pathname);

  useEffect(() => {
    if (previousPathname.current === pathname) return;
    previousPathname.current = pathname;
    document.getElementById(MAIN_CONTENT_ID)?.focus({ preventScroll: true });
  }, [pathname]);

  return null;
}
