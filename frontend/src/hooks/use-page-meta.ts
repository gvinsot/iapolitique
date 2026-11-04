import { useEffect } from "react";

/**
 * Per-page metadata of the single-page app (search engines that render JavaScript). index.html
 * carries the site-wide defaults, which are also what social crawlers (no JavaScript) show; a
 * page replaces them while it is displayed and puts them back when it goes away, so there is
 * never more than one description, Open Graph tag or canonical link in the document.
 */

/**
 * Sets the `content` of `<meta {attribute}="{key}">` (created if missing) and returns the
 * function that restores the previous state.
 */
function setMeta(attribute: "name" | "property", key: string, content: string): () => void {
  let meta = document.head.querySelector<HTMLMetaElement>(`meta[${attribute}="${key}"]`);
  const created = meta === null;
  if (!meta) {
    meta = document.createElement("meta");
    meta.setAttribute(attribute, key);
    document.head.append(meta);
  }
  const previous = meta.content;
  meta.content = content;
  const element = meta;
  return () => {
    if (created) element.remove();
    else element.content = previous;
  };
}

/** Sets `og:title` (link previews) to the full document title of the page. */
export function useOpenGraphTitle(title: string): void {
  useEffect(() => setMeta("property", "og:title", title), [title]);
}

/** Sets `<meta name="description">` and `og:description` (search engines, link previews). */
export function usePageDescription(description: string | null | undefined): void {
  useEffect(() => {
    if (!description) return;
    const restore = [
      setMeta("name", "description", description),
      setMeta("property", "og:description", description),
    ];
    return () => {
      for (const undo of restore) undo();
    };
  }, [description]);
}

/**
 * Adds `<link rel="canonical">` and sets `og:url` (absolute `http(s)` URL) while the page is
 * shown.
 */
export function useCanonicalLink(url: string | null | undefined): void {
  useEffect(() => {
    if (!url || !/^https?:\/\//.test(url)) return;
    const link = document.createElement("link");
    link.rel = "canonical";
    link.href = url;
    document.head.append(link);
    const restoreUrl = setMeta("property", "og:url", url);
    return () => {
      link.remove();
      restoreUrl();
    };
  }, [url]);
}

/**
 * Canonical link of a page of the portal at `path` (from `paths`) on the current origin: the
 * query string (filters, sort, page of the ranking) and the fragment are left out.
 */
export function useCanonicalPath(path: string): void {
  useCanonicalLink(`${window.location.origin}${path}`);
}
