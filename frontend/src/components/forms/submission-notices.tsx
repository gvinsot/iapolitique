import { BotIcon, UserRoundXIcon } from "lucide-react";
import { Link } from "react-router";

import { paths } from "@/app/paths";

const linkClass = "font-medium text-primary underline underline-offset-4 hover:no-underline";

/** Warning shown next to every free-text field (AGENTS.md §7.5). */
export function PersonalDataWarning({ id }: { id?: string }) {
  return (
    <p id={id} className="flex items-start gap-2 text-sm font-medium">
      <UserRoundXIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      Ne signez pas, ne nommez aucun agent ni particulier.
    </p>
  );
}

/** How a publishable submission is processed by AI (AGENTS.md §7.5). */
export function AiProcessingNotice() {
  return (
    <p className="flex items-start gap-2 text-sm text-muted-foreground">
      <BotIcon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <span>
        Votre proposition sera évaluée automatiquement par une IA (modèle local, puis Mistral AI) ;
        si elle est acceptée, elle sera reformulée et analysée par IA.
      </span>
    </p>
  );
}

/** Links to the rules and the privacy policy, under every form. */
export function FormLegalLinks() {
  return (
    <p className="text-sm text-muted-foreground">
      En savoir plus :{" "}
      <Link to={paths.terms} className={linkClass}>
        règles du site
      </Link>{" "}
      et{" "}
      <Link to={paths.privacy} className={linkClass}>
        politique de confidentialité
      </Link>
      .
    </p>
  );
}
