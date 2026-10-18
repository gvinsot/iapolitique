import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { Link } from "react-router";

import { buttonVariants } from "@/components/ui/button";
import { paginationItems } from "@/lib/pagination";
import { cn } from "@/lib/utils";

interface PaginationProps {
  page: number;
  pageCount: number;
  /** URL of a page (filters are kept in the query string). */
  hrefForPage: (page: number) => string;
  className?: string;
}

const linkClass = cn(buttonVariants({ variant: "outline", size: "icon" }), "w-auto min-w-9 px-3");

/** Link-based pagination: every page has a shareable URL and works without JavaScript state. */
export function Pagination({ page, pageCount, hrefForPage, className }: PaginationProps) {
  if (pageCount <= 1) return null;
  const current = Math.min(Math.max(1, page), pageCount);

  return (
    <nav aria-label="Pagination" className={cn("flex justify-center", className)}>
      <ul className="flex flex-wrap items-center gap-1">
        <li>
          {current > 1 ? (
            <Link to={hrefForPage(current - 1)} className={linkClass} rel="prev">
              <ChevronLeftIcon aria-hidden="true" />
              <span className="sr-only sm:not-sr-only">Précédente</span>
            </Link>
          ) : null}
        </li>
        {paginationItems(current, pageCount).map((item, index) =>
          item === null ? (
            <li key={`gap-${index}`} aria-hidden="true" className="px-2 text-muted-foreground">
              …
            </li>
          ) : (
            <li key={item}>
              <Link
                to={hrefForPage(item)}
                aria-current={item === current ? "page" : undefined}
                aria-label={`Page ${item}`}
                className={cn(
                  linkClass,
                  item === current &&
                    "border-primary bg-primary text-primary-foreground hover:bg-primary/90 hover:text-primary-foreground",
                )}
              >
                {item}
              </Link>
            </li>
          ),
        )}
        <li>
          {current < pageCount ? (
            <Link to={hrefForPage(current + 1)} className={linkClass} rel="next">
              <span className="sr-only sm:not-sr-only">Suivante</span>
              <ChevronRightIcon aria-hidden="true" />
            </Link>
          ) : null}
        </li>
      </ul>
    </nav>
  );
}
