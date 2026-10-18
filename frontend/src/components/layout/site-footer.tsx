import { BotIcon, ShieldCheckIcon } from "lucide-react";
import { Link } from "react-router";

import { paths, SITE_NAME } from "@/app/paths";
import { Separator } from "@/components/ui/separator";

import { Container } from "./container";
import { legalNavItems } from "./nav-items";

const linkClass =
  "rounded-sm text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline";

export function SiteFooter() {
  return (
    <footer className="mt-16 border-t bg-muted/40">
      <Container className="grid gap-8 py-10 sm:grid-cols-2 lg:grid-cols-4">
        <div className="space-y-2 sm:col-span-2">
          <p className="font-semibold">{SITE_NAME}</p>
          <p className="max-w-md text-sm text-muted-foreground">
            Une plateforme indépendante et apolitique qui recueille des idées anonymes pour réduire
            les dépenses ou augmenter les recettes publiques, puis les fait analyser, chiffrer et
            classer par intelligence artificielle.
          </p>
        </div>

        <nav aria-label="Comprendre la démarche">
          <h2 className="mb-3 text-sm font-semibold">Comprendre</h2>
          <ul className="space-y-2">
            <li>
              <Link to={paths.about} className={linkClass}>
                À propos et fonctionnement
              </Link>
            </li>
            <li>
              <Link to={paths.methodology} className={linkClass}>
                Méthodologie et prompts
              </Link>
            </li>
          </ul>
        </nav>

        <nav aria-label="Informations légales">
          <h2 className="mb-3 text-sm font-semibold">Informations légales</h2>
          <ul className="space-y-2">
            {legalNavItems.map((item) => (
              <li key={item.to}>
                <Link to={item.to} className={linkClass}>
                  {item.label}
                </Link>
              </li>
            ))}
          </ul>
        </nav>
      </Container>

      <Container>
        <Separator />
        <div className="flex flex-col gap-3 py-6 text-sm text-muted-foreground">
          <p className="flex items-start gap-2">
            <BotIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
            <span>
              <strong className="font-semibold text-foreground">Analyses générées par IA.</strong>{" "}
              Le filtrage, les reformulations et les chiffrages sont produits automatiquement et
              peuvent comporter des erreurs : vérifiez toujours les sources citées.
            </span>
          </p>
          <p className="flex items-start gap-2">
            <ShieldCheckIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
            <span>
              <strong className="font-semibold text-foreground">Site non officiel</strong>, sans
              lien avec l’État, une administration publique ou un parti politique. Aucun compte,
              aucun cookie, aucun traceur.
            </span>
          </p>
        </div>
      </Container>
    </footer>
  );
}
