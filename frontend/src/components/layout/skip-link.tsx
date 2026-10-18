export const MAIN_CONTENT_ID = "contenu";

/** First focusable element of every page (RGAA 12.7): jumps over the header navigation. */
export function SkipLink() {
  return (
    <a
      href={`#${MAIN_CONTENT_ID}`}
      className="sr-only rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-100"
    >
      Aller au contenu principal
    </a>
  );
}
