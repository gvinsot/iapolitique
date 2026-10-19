import { useEffect } from "react";

/**
 * Per-page metadata of the single-page app. index.html carries the site-wide defaults; a page
 * replaces them while it is displayed and puts them back when it goes away, so there is never
 * more than one description or canonical link in the document.
 */

const DESCRIPTION_SELECTOR = 'meta[name="description"]';

/** Sets the `<meta name="description">` of the page (search engines, link previews). */
export function usePageDescription(description: string | null | undefined): void {
  useEffect(() => {
    if (!description) return;
    let meta = document.head.querySelector<HTMLMetaElement>(DESCRIPTION_SELECTOR);
    const created = meta === null;
    if (!meta) {
      meta = document.createElement("meta");
      meta.name = "description";
      document.head.append(meta);
    }
    const previous = meta.content;
    meta.content = description;
    return () => {
      if (created) meta.remove();
      else meta.content = previous;
    };
  }, [description]);
}

/** Adds `<link rel="canonical">` (absolute URL given by the API) while the page is shown. */
export function useCanonicalLink(url: string | null | undefined): void {
  useEffect(() => {
    if (!url || !/^https?:\/\//.test(url)) return;
    const link = document.createElement("link");
    link.rel = "canonical";
    link.href = url;
    document.head.append(link);
    return () => {
      link.remove();
    };
  }, [url]);
}
