import { MenuIcon, XIcon } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { Link, NavLink } from "react-router";

import { paths, SITE_NAME, SITE_TAGLINE } from "@/app/paths";
import { ThemeToggle } from "@/components/theme/theme-toggle";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { BrandMark } from "./brand-mark";
import { Container } from "./container";
import { mainNavItems } from "./nav-items";

const desktopLinkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "rounded-md px-3 py-2 text-sm font-medium transition-colors hover:bg-accent hover:text-accent-foreground",
    isActive
      ? "text-foreground underline decoration-primary decoration-2 underline-offset-8"
      : "text-muted-foreground",
  );

const mobileLinkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "block rounded-md px-3 py-3 text-base font-medium hover:bg-accent hover:text-accent-foreground",
    isActive ? "bg-secondary text-foreground" : "text-muted-foreground",
  );

export function SiteHeader() {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuId = useId();

  useEffect(() => {
    if (!menuOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenuOpen(false);
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [menuOpen]);

  const closeMenu = () => {
    setMenuOpen(false);
  };

  return (
    <header className="sticky top-0 z-40 border-b bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/80">
      <Container className="flex min-h-16 items-center gap-3 py-2">
        <Link
          to={paths.home}
          className="flex min-w-0 items-center gap-3 rounded-md"
          onClick={closeMenu}
        >
          <BrandMark />
          <span className="flex min-w-0 flex-col">
            <span className="text-lg leading-tight font-semibold tracking-tight">{SITE_NAME}</span>
            <span className="text-xs leading-snug text-pretty text-muted-foreground">
              {SITE_TAGLINE}
            </span>
          </span>
        </Link>

        <nav aria-label="Navigation principale" className="ml-auto hidden lg:block">
          <ul className="flex items-center gap-1">
            {mainNavItems.map((item) => (
              <li key={item.to}>
                <NavLink to={item.to} end={item.end ?? false} className={desktopLinkClass}>
                  {item.label}
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>

        <div className="ml-auto flex shrink-0 items-center gap-1 lg:ml-2">
          <ThemeToggle />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="lg:hidden"
            aria-expanded={menuOpen}
            aria-controls={menuId}
            aria-label={menuOpen ? "Fermer le menu" : "Ouvrir le menu"}
            onClick={() => {
              setMenuOpen((open) => !open);
            }}
          >
            {menuOpen ? <XIcon aria-hidden="true" /> : <MenuIcon aria-hidden="true" />}
          </Button>
        </div>
      </Container>

      <nav
        id={menuId}
        aria-label="Navigation principale (mobile)"
        hidden={!menuOpen}
        className="border-t lg:hidden"
      >
        <Container>
          <ul className="flex flex-col py-2">
            {mainNavItems.map((item) => (
              <li key={item.to}>
                <NavLink
                  to={item.to}
                  end={item.end ?? false}
                  onClick={closeMenu}
                  className={mobileLinkClass}
                >
                  {item.label}
                </NavLink>
              </li>
            ))}
          </ul>
        </Container>
      </nav>
    </header>
  );
}
