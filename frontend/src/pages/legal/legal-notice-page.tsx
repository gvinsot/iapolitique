import { Link } from "react-router";

import { paths } from "@/app/paths";
import { LegalEmail, LegalValue } from "@/components/legal/legal-details";
import { useLegalDetails } from "@/hooks/use-legal-info";
import { PHAROS_URL, SITE_DOMAIN, SOURCE_CODE_URL } from "@/lib/legal";
import { RULES_RECOURSE_ANCHOR } from "@/lib/rules";

import { LegalPage, type TocEntry } from "./legal-page";

const toc: readonly TocEntry[] = [
  { anchor: "editeur", label: "1. Éditeur" },
  { anchor: "directeur-publication", label: "2. Directeur de la publication" },
  { anchor: "hebergement", label: "3. Hébergement" },
  { anchor: "site-non-officiel", label: "4. Site non officiel" },
  { anchor: "contenus-ia", label: "5. Contenus générés par IA" },
  { anchor: "contributions", label: "6. Contributions des internautes" },
  { anchor: "signalement", label: "7. Signaler un contenu" },
  { anchor: "droit-de-reponse", label: "8. Droit de réponse" },
  { anchor: "propriete-intellectuelle", label: "9. Propriété intellectuelle" },
  { anchor: "donnees-personnelles", label: "10. Données personnelles" },
  { anchor: "droit-applicable", label: "11. Droit applicable" },
];

/** Mentions légales (LCEN art. 1-1, option A of docs/juridique/memo.md §2). */
export function LegalNoticePage() {
  const details = useLegalDetails();
  const { info, pending } = details;
  const publisherName = info?.publisher.name;

  return (
    <LegalPage
      title="Mentions légales"
      description="Qui édite et héberge ce site, et comment le contacter."
      pageKey="mentions_legales"
      details={details}
      toc={toc}
    >
      <h2 id="editeur">1. Éditeur</h2>
      <p>
        {SITE_DOMAIN} est édité à titre non professionnel par{" "}
        <LegalValue value={publisherName} pending={pending} />, demeurant{" "}
        <LegalValue value={info?.publisher.address} pending={pending} />, téléphone :{" "}
        <LegalValue value={info?.publisher.phone} pending={pending} />, courriel :{" "}
        <LegalEmail email={info?.contact_email} pending={pending} />.
      </p>

      <h2 id="directeur-publication">2. Directeur de la publication</h2>
      <p>
        <LegalValue value={publisherName} pending={pending} />.
      </p>

      <h2 id="hebergement">3. Hébergement</h2>
      <p>
        Le site est auto-hébergé par l’éditeur sur ses propres serveurs, situés en France
        (coordonnées ci-dessus).
      </p>
      <p>
        Le modèle de modération est exécuté sur les serveurs de l’éditeur. L’analyse approfondie des
        idées est réalisée par Mistral AI, en qualité de sous-traitant (voir la{" "}
        <Link to={paths.privacy}>politique de confidentialité</Link>).
      </p>

      <h2 id="site-non-officiel">4. Site non officiel</h2>
      <p>
        {SITE_DOMAIN} est une initiative citoyenne indépendante, non commerciale et apartisane. Il
        n’est ni édité, ni financé, ni approuvé par l’État, une administration, une collectivité
        territoriale, un organisme de sécurité sociale, un parti politique ou Mistral AI.
      </p>
      <p>
        La publication d’une idée ne constitue ni une pétition, ni une consultation publique, ni une
        saisine d’une administration. Le score de classement est calculé par un algorithme : ce
        n’est ni un sondage, ni une mesure d’opinion.
      </p>

      <h2 id="contenus-ia">5. Contenus générés par intelligence artificielle</h2>
      <p>
        Les reformulations, propositions améliorées, avantages et inconvénients, impacts,
        estimations d’économies ou de recettes et classements sont générés automatiquement par
        intelligence artificielle, sans relecture humaine. Ils peuvent contenir des erreurs. Les
        estimations sont indicatives, ne sont pas des chiffrages officiels et doivent être vérifiées
        à partir des sources citées.
      </p>

      <h2 id="contributions">6. Contributions des internautes</h2>
      <p>
        Le texte original de chaque idée est publié dans un espace « Contribution d’un internaute »,
        après filtrage automatisé et masquage des données personnelles. Les règles de publication et
        de modération sont décrites dans les <Link to={paths.terms}>règles du site</Link>.
      </p>

      <h2 id="signalement">7. Signaler un contenu – point de contact</h2>
      <p>
        Chaque idée publiée dispose d’un bouton « Signaler ». Vous pouvez aussi écrire à{" "}
        <LegalEmail email={info?.contact_email} pending={pending} /> : cette adresse est lue par un
        humain et constitue le point de contact unique pour les autorités et les utilisateurs
        (règlement (UE) 2022/2065 sur les services numériques, articles 11 et 12 ; langue :
        français). Indiquez le motif, l’adresse de la page concernée et, si vous le souhaitez, vos
        nom et adresse e-mail.
      </p>
      <p>
        Le premier traitement d’un signalement est automatisé. Les contenus graves (menace, haine,
        terrorisme, atteinte aux mineurs…) sont masqués et examinés par un humain, qui les transmet
        si nécessaire à la plateforme officielle PHAROS (
        <a href={PHAROS_URL} rel="noreferrer">
          internet-signalement.gouv.fr
        </a>
        ), à laquelle vous pouvez aussi vous adresser directement.
      </p>
      <p>
        Présenter sciemment un contenu comme illicite dans le but d’en obtenir le retrait ou d’en
        faire cesser la diffusion est puni d’un an d’emprisonnement et de 15 000 € d’amende (article
        6, VII de la loi n° 2004-575 du 21 juin 2004 pour la confiance dans l’économie numérique).
      </p>

      <h2 id="droit-de-reponse">8. Droit de réponse</h2>
      <p>
        Toute personne nommée ou désignée sur le site dispose d’un droit de réponse (article 1-1,
        III de la loi du 21 juin 2004), à exercer dans les trois mois suivant la mise en ligne du
        contenu, par une demande adressée au directeur de la publication à{" "}
        <LegalEmail email={info?.contact_email} pending={pending} />, dans les formes prévues par le
        décret n° 2007-1527 du 24 octobre 2007. La réponse est insérée dans les trois jours suivant
        la réception de la demande.
      </p>

      <h2 id="propriete-intellectuelle">9. Propriété intellectuelle et réutilisation</h2>
      <p>
        Le code source du site est publié sous licence MIT :{" "}
        <a href={SOURCE_CODE_URL} rel="noreferrer">
          dépôt du code source
        </a>
        .
      </p>
      <p>
        Les données exportées sont diffusées sous Licence Ouverte 2.0 (Etalab), dans la mesure où
        des droits existent ; les contenus de tiers cités en sont exclus. Voir la page{" "}
        <Link to={paths.licence}>Licence et réutilisation</Link>.
      </p>

      <h2 id="donnees-personnelles">10. Données personnelles</h2>
      <p>
        Voir la <Link to={paths.privacy}>politique de confidentialité</Link>. Les voies de recours
        contre une décision de modération sont décrites dans les{" "}
        <Link to={`${paths.terms}#${RULES_RECOURSE_ANCHOR}`}>règles du site</Link>.
      </p>

      <h2 id="droit-applicable">11. Droit applicable</h2>
      <p>Le site et les présentes mentions sont régis par le droit français.</p>
    </LegalPage>
  );
}
