import { DownloadIcon } from "lucide-react";
import { Link } from "react-router";

import { paths } from "@/app/paths";
import { CopyButton } from "@/components/common/copy-button";
import { Button } from "@/components/ui/button";
import { useExportStatus } from "@/hooks/use-export-metadata";
import { useLegalDetails } from "@/hooks/use-legal-info";
import { EXPORT_URLS } from "@/lib/exports";
import { formatDate, formatInteger } from "@/lib/format";
import {
  AI_REUSE_REQUEST,
  ATTRIBUTION_DATE_PLACEHOLDER,
  attributionText,
  CC_BY_URL,
  CONTRIBUTOR_ATTRIBUTION,
  LICENCE_NAME,
  LICENCE_SPDX,
  LICENCE_URL,
  SITE_DOMAIN,
} from "@/lib/legal";
import { RULES_LICENCE_ANCHOR } from "@/lib/rules";

import { LegalPage, type TocEntry } from "./legal-page";

const toc: readonly TocEntry[] = [
  { anchor: "licence", label: "1. La licence" },
  { anchor: "telecharger", label: "2. Télécharger les données" },
  { anchor: "attribution", label: "3. Citer la source" },
  { anchor: "avertissement-ia", label: "4. Contenus générés par IA" },
  { anchor: "limites", label: "5. Limites de la licence" },
  { anchor: "irrevocabilite", label: "6. Copies déjà diffusées" },
];

interface LicenceAttributionProps {
  /** `YYYY-MM-DD` date of the export being reused; a placeholder is shown when unknown. */
  updatedOn?: string | null;
}

/** Attribution text ready to copy, with the AI reminder to add (AGENTS.md §7.4). */
export function LicenceAttribution({ updatedOn }: LicenceAttributionProps) {
  const text = attributionText(updatedOn);
  const withReminder = `${text}. ${AI_REUSE_REQUEST}`;
  return (
    <figure className="my-4 space-y-3 rounded-lg border bg-card p-4">
      <figcaption className="text-sm font-semibold">Mention à reproduire</figcaption>
      <blockquote className="font-medium">
        <p>{text}</p>
        <p className="mt-1 text-sm font-normal text-muted-foreground">{AI_REUSE_REQUEST}</p>
      </blockquote>
      <CopyButton value={withReminder} label="Copier la mention" size="sm" />
    </figure>
  );
}

/** Licence and reuse of the exported data (AGENTS.md §7.4, docs/juridique/memo.md §1). */
/** Date and size of the latest export, or why the files are not there yet. */
function ExportFreshness() {
  const status = useExportStatus();
  if (status.pending) return null;
  if (status.unavailable || !status.generatedOn) {
    return (
      <p role="status">
        Les fichiers ne sont pas disponibles pour le moment : ils sont générés après la publication
        des premières idées, puis chaque nuit. Réessayez plus tard.
      </p>
    );
  }
  return (
    <p role="status">
      Dernière génération :{" "}
      <time dateTime={status.generatedOn}>{formatDate(status.generatedOn)}</time>
      {status.ideaCount !== null
        ? ` (${formatInteger(status.ideaCount)} idée${status.ideaCount > 1 ? "s" : ""} publiée${status.ideaCount > 1 ? "s" : ""})`
        : null}
      .
    </p>
  );
}

export function LicencePage() {
  const details = useLegalDetails();
  const { generatedOn } = useExportStatus();

  return (
    <LegalPage
      title="Licence et réutilisation"
      description="Les idées publiées et leurs analyses sont des données ouvertes : téléchargez-les et réutilisez-les librement, en citant la source."
      pageKey="licence"
      details={details}
      toc={toc}
    >
      <h2 id="licence">1. La licence</h2>
      <p>
        Les données exportées par {SITE_DOMAIN} sont diffusées sous{" "}
        <a href={LICENCE_URL} rel="noreferrer">
          {LICENCE_NAME}
        </a>{" "}
        (identifiant SPDX <code>{LICENCE_SPDX}</code>), dans la mesure où des droits existent. Vous
        pouvez les reproduire, les copier, les adapter, les modifier, les extraire, les transformer
        et les exploiter, y compris à des fins commerciales, à condition de mentionner leur source
        et la date de leur dernière mise à jour, sans induire en erreur sur leur contenu, leur
        source ou leur date.
      </p>
      <p>
        Cette licence est compatible avec la licence{" "}
        <a href={CC_BY_URL} rel="noreferrer">
          Creative Commons Attribution 4.0 (CC BY 4.0)
        </a>
        . Le concédant est l’éditeur d’{SITE_DOMAIN}, à qui les auteurs des textes ont concédé leurs
        droits (voir l’
        <Link to={`${paths.terms}#${RULES_LICENCE_ANCHOR}`}>article 11 des règles</Link>).
      </p>

      <h2 id="telecharger">2. Télécharger les données</h2>
      <p>
        Les fichiers sont régénérés chaque nuit et après chaque masquage ou suppression. Chacun
        contient le texte de la licence et des métadonnées : date de génération, version du format,
        avertissement sur les contenus générés par IA.
      </p>
      <div className="mb-4 flex flex-wrap gap-3">
        <Button asChild variant="outline">
          <a href={EXPORT_URLS.json} download>
            <DownloadIcon aria-hidden="true" />
            Télécharger au format JSON
          </a>
        </Button>
        <Button asChild variant="outline">
          <a href={EXPORT_URLS.csv} download>
            <DownloadIcon aria-hidden="true" />
            Télécharger au format CSV
          </a>
        </Button>
      </div>
      <ExportFreshness />
      <p>
        Également disponibles séparément : le{" "}
        <a href={EXPORT_URLS.license} download>
          texte de la licence
        </a>{" "}
        et les{" "}
        <a href={EXPORT_URLS.metadata} download>
          métadonnées de l’export
        </a>{" "}
        (date de génération, version du format, règles du fichier CSV).
      </p>

      <h2 id="attribution">3. Citer la source</h2>
      {generatedOn ? (
        <p>
          Reproduisez la mention suivante, datée du dernier export. Si vous utilisez un fichier plus
          ancien, remplacez la date par celle de ses métadonnées (champ <code>generated_at</code>).
        </p>
      ) : (
        <p>
          Reproduisez la mention suivante en remplaçant {ATTRIBUTION_DATE_PLACEHOLDER} par la date
          de mise à jour indiquée dans les métadonnées du fichier utilisé (champ{" "}
          <code>generated_at</code>).
        </p>
      )}
      <LicenceAttribution updatedOn={generatedOn} />
      <p>
        La seconde phrase est une demande, pas une condition supplémentaire de la licence. Les
        textes originaux des internautes sont attribués sous la forme « {CONTRIBUTOR_ATTRIBUTION} ».
      </p>

      <h2 id="avertissement-ia">4. Contenus générés par IA</h2>
      <p>
        Les reformulations, analyses, impacts et chiffrages ont été générés automatiquement par
        intelligence artificielle (Mistral AI), sans relecture humaine. Ils peuvent contenir des
        erreurs et ne constituent ni un chiffrage officiel, ni une position officielle. Dans les
        exports, ces champs sont marqués comme générés par IA, avec le modèle, la date et la version
        de l’analyse. Ne les présentez pas comme l’avis d’une administration ou comme le résultat
        d’un sondage.
      </p>

      <h2 id="limites">5. Limites de la licence</h2>
      <ul>
        <li>
          Les champs générés par IA, produits sans intervention humaine significative, ne sont
          probablement protégés par aucun droit d’auteur : la licence ne s’y applique que dans la
          mesure où des droits existent.
        </li>
        <li>
          Les contenus de tiers cités (titres de documents, publications) sont exclus de la licence
          : les exports ne contiennent que des références (titre, éditeur, adresse, date).
        </li>
        <li>
          Les données personnelles ont été masquées avant publication. Si vous en repérez malgré
          tout, signalez-le depuis la page de l’idée concernée.
        </li>
      </ul>

      <h2 id="irrevocabilite">6. Copies déjà diffusées</h2>
      <p>
        Une idée retirée du site (masquée ou supprimée par son auteur) disparaît des exports
        suivants, mais pas des copies déjà téléchargées. Si vous réutilisez ces données, merci de
        vous resynchroniser régulièrement avec la dernière version des fichiers.
      </p>
    </LegalPage>
  );
}
