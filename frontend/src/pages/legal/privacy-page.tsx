import type { ReactNode } from "react";
import { Link } from "react-router";

import { ADMIN_TOKEN_KEY } from "@/api/admin-token";
import { paths } from "@/app/paths";
import { AudienceOptOut } from "@/components/legal/audience-opt-out";
import { LegalEmail, LegalValue } from "@/components/legal/legal-details";
import { useLegalDetails } from "@/hooks/use-legal-info";
import { AUDIENCE_OPT_OUT_KEY } from "@/lib/audience";
import { CNIL_COMPLAINT_URL } from "@/lib/legal";
import { RULES_LICENCE_ANCHOR, RULES_RECOURSE_ANCHOR } from "@/lib/rules";
import { THEME_STORAGE_KEY } from "@/lib/theme";
import { TRACKING_LINKS_STORAGE_KEY } from "@/lib/tracking-token";

import { LegalPage, type TocEntry } from "./legal-page";

const toc: readonly TocEntry[] = [
  { anchor: "responsable", label: "1. Responsable du traitement" },
  { anchor: "principes", label: "2. Nos principes" },
  { anchor: "traitements", label: "3. Données, finalités et durées" },
  { anchor: "consignes", label: "4. Consignes aux auteurs" },
  { anchor: "ia", label: "5. Intelligence artificielle" },
  { anchor: "destinataires", label: "6. Destinataires" },
  { anchor: "open-data", label: "7. Données ouvertes" },
  { anchor: "stockage-local", label: "8. Stockage dans le navigateur" },
  { anchor: "securite", label: "9. Sécurité et sauvegardes" },
  { anchor: "droits", label: "10. Vos droits" },
];

interface Processing {
  title: string;
  data: ReactNode;
  purpose: ReactNode;
  legalBasis: ReactNode;
  retention: ReactNode;
}

const processings: readonly Processing[] = [
  {
    title: "Textes soumis (idées, améliorations, objections)",
    data: "Le texte que vous écrivez, le niveau administratif et le territoire facultatifs, la date d’envoi, la version des règles acceptée et la date d’acceptation (au jour près). Aucune donnée d’identification n’y est associée.",
    purpose:
      "Filtrer automatiquement le texte, l’analyser et le chiffrer, puis le publier s’il est accepté.",
    legalBasis: (
      <>
        Intérêt légitime de l’éditeur à faire vivre un débat public sur les finances publiques
        (article 6, 1, f du RGPD). Si votre texte révèle indirectement une opinion politique, sa
        publication repose sur votre choix explicite de le rendre public, matérialisé par la case «
        je comprends que mon texte sera publié s’il est accepté » (article 9, 2, e du RGPD).
      </>
    ),
    retention: (
      <>
        <ul>
          <li>texte refusé : supprimé 30 jours après le refus, avec son lien de suivi ;</li>
          <li>texte publié : conservé pendant toute la durée de vie du site ;</li>
          <li>texte masqué après un signalement : supprimé 12 mois après le masquage ;</li>
          <li>
            idée publiée (ou masquée) supprimée par son auteur : retirée immédiatement du site, de
            l’API publique et des exports suivants, avec son analyse, ses contributions, ses
            signalements et ses droits de réponse ; elle n’est plus accessible qu’à l’éditeur,
            marquée « retirée par son auteur », puis supprimée 12 mois après le retrait ;
          </li>
          <li>
            autre texte supprimé par son auteur (texte non publié, refusé ou en cours d’examen,
            amélioration, objection) : effacé immédiatement.
          </li>
        </ul>
        <p>
          La conservation d’une idée retirée pendant 12 mois repose sur l’intérêt légitime de
          l’éditeur à pouvoir répondre d’un contenu qu’il a publié (réclamation, droit de réponse,
          demande d’une autorité), comme pour un contenu masqué.
        </p>
      </>
    ),
  },
  {
    title: "Lien de suivi",
    data: "Une empreinte cryptographique (SHA-256) du jeton de votre lien de suivi. Le lien lui-même n’est jamais enregistré par le site.",
    purpose:
      "Vous permettre de suivre votre envoi, de le supprimer ou de demander un réexamen, sans compte.",
    legalBasis: "Intérêt légitime (article 6, 1, f du RGPD).",
    retention:
      "Supprimée en même temps que le texte auquel elle se rapporte. Après une suppression à votre demande, elle sert seulement, pendant 30 jours, à afficher que la suppression est faite.",
  },
  {
    title: "Signalements",
    data: "Le motif, la précision que vous donnez et, si vous choisissez de les indiquer, vos nom et adresse e-mail.",
    purpose:
      "Examiner le signalement, corriger ou masquer le contenu en cause et, pour les contenus graves, les transmettre aux autorités.",
    legalBasis:
      "Intérêt légitime à modérer le site et respect des obligations de la loi pour la confiance dans l’économie numérique (article 6, 1, c et f du RGPD).",
    retention:
      "Supprimés 6 mois après leur traitement, coordonnées facultatives comprises (ou avec l’idée signalée, si elle est supprimée avant). Le motif et la précision d’un signalement à l’origine d’une escalade figurent aussi dans la copie scellée décrite ci-dessous, jamais vos nom et adresse e-mail.",
  },
  {
    title: "Copies scellées des contenus escaladés",
    data: "Pour un contenu jugé grave à l’examen d’un signalement (diffamation, menace, haine, terrorisme, atteinte aux mineurs) : une copie du texte original masqué, du titre, du résumé, du territoire, des dates de dépôt, de publication et de masquage, de l’analyse, de l’avis rendu sur les signalements, et du motif et de la précision des signalements qui l’ont déclenché. Jamais le nom ni l’adresse e-mail des signalants, ni aucune donnée d’identification de l’auteur.",
    purpose:
      "Garder la preuve d’un contenu grave masqué et, le cas échéant, transmis aux autorités (plateforme PHAROS), même si son auteur le supprime ensuite, pour répondre aux réquisitions et pour la constatation, l’exercice ou la défense de droits en justice.",
    legalBasis: (
      <>
        Respect des obligations de la loi pour la confiance dans l’économie numérique (LCEN) et
        intérêt légitime de l’éditeur à la constatation, à l’exercice ou à la défense de droits en
        justice (article 6, 1, c et f du RGPD). Pendant cette durée, le droit à l’effacement ne
        s’applique pas à la copie (article 17, 3, e du RGPD) : la suppression d’un contenu escaladé
        par son auteur le retire du site, mais pas de la copie scellée.
      </>
    ),
    retention:
      "1 an après l’escalade, puis suppression automatique. La copie n’est accessible qu’à l’éditeur, chaque consultation est journalisée, et elle n’est jamais publiée ni exportée.",
  },
  {
    title: "Demandes de réexamen humain",
    data: "Le message facultatif que vous joignez à votre demande.",
    purpose: "Faire réexaminer par un humain une décision prise automatiquement.",
    legalBasis: "Intérêt légitime (article 6, 1, f du RGPD).",
    retention:
      "Conservé avec le texte concerné, supprimé en même temps que lui et au plus tard 1 an après le traitement de la demande.",
  },
  {
    title: "Protection contre les abus",
    data: (
      <>
        Une vérification anti-robot (preuve de travail calculée par votre navigateur, sans cookie ni
        service tiers) et, pour limiter le nombre d’envois, une empreinte de votre adresse IP
        calculée avec une clé aléatoire gardée uniquement en mémoire et renouvelée toutes les 24
        heures. Il s’agit d’une pseudonymisation éphémère : votre adresse IP n’est jamais
        enregistrée, et l’empreinte n’est jamais associée à un texte.
      </>
    ),
    purpose: "Empêcher les envois massifs automatisés et les abus.",
    legalBasis: "Intérêt légitime à assurer la sécurité du service (article 6, 1, f du RGPD).",
    retention: "24 heures au plus, en mémoire uniquement, puis destruction de la clé.",
  },
  {
    title: "Journaux techniques",
    data: "L’application et son serveur web ne journalisent ni adresse IP, ni navigateur, ni lien de suivi. Le serveur mandataire qui reçoit les connexions tronque les adresses IP de ses journaux ou ne les conserve pas plus de 7 jours.",
    purpose: "Assurer le bon fonctionnement et la sécurité du site.",
    legalBasis: "Intérêt légitime (article 6, 1, f du RGPD).",
    retention: "7 jours au plus.",
  },
  {
    title: "Mesure d’audience",
    data: (
      <>
        Pour chaque page vue : son adresse sans paramètres ni fragment (le lien de suivi n’est donc
        jamais transmis), son titre, le site d’où vous venez, la langue, la taille d’écran, le type
        de navigateur et d’appareil et le pays déduit de l’adresse IP. L’adresse IP sert à calculer
        un identifiant de visite non réversible, renouvelé périodiquement, et n’est pas conservée.
        Aucun cookie n’est déposé.
      </>
    ),
    purpose:
      "Produire des statistiques anonymes de fréquentation (nombre de visites, pages consultées), exclusivement pour l’éditeur.",
    legalBasis: (
      <>
        Intérêt légitime de l’éditeur à connaître la fréquentation du site (article 6, 1, f du
        RGPD). Outil de mesure d’audience auto-hébergé, sans cookie, limité à des statistiques
        anonymes et jamais croisé avec d’autres données. La mesure est désactivée si votre
        navigateur envoie le signal « Do Not Track », et vous pouvez la refuser à tout moment (voir{" "}
        <a href="#mesure-audience">ci-dessous</a>).
      </>
    ),
    retention: "25 mois au plus.",
  },
  {
    title: "Demandes d’exercice de vos droits",
    data: "Votre message et l’adresse e-mail que vous utilisez pour nous écrire.",
    purpose: "Répondre à votre demande et en garder la preuve.",
    legalBasis: "Obligation légale (article 6, 1, c du RGPD).",
    retention: "1 an après la réponse.",
  },
];

/** Summary of the retention periods of section 3 (AGENTS.md §7.3), data → retention. */
const retentionRows: readonly (readonly [string, string])[] = [
  ["Texte refusé et son lien de suivi", "30 jours après le refus"],
  ["Idée publiée et son analyse", "Durée de vie du site"],
  ["Idée masquée (signalement ou éditeur)", "12 mois après le masquage"],
  [
    "Idée publiée ou masquée supprimée par son auteur (retirée)",
    "Retirée aussitôt du site, de l’API et des exports ; visible de l’éditeur seul ; supprimée 12 mois après le retrait",
  ],
  [
    "Texte non publié, amélioration ou objection supprimés par leur auteur",
    "Effacés immédiatement",
  ],
  [
    "Copie scellée d’un contenu escaladé",
    "1 an après l’escalade, éditeur seul (RGPD art. 17, 3, e ; LCEN)",
  ],
  ["Nom et e-mail facultatifs d’un signalant", "6 mois après le signalement"],
  ["Précision d’un signalement", "6 mois après son traitement"],
  ["Demande de réexamen humain", "Avec le texte, et 1 an au plus après son traitement"],
  ["Empreinte anti-abus de l’adresse IP", "24 heures, en mémoire uniquement"],
  ["Journaux techniques", "7 jours au plus"],
  ["Statistiques de mesure d’audience", "25 mois au plus"],
  ["Sauvegardes de la base", "30 jours au plus"],
];

/** Politique de confidentialité (RGPD art. 13, plan of docs/juridique/memo.md §3). */
export function PrivacyPage() {
  const details = useLegalDetails();
  const { info, pending } = details;

  return (
    <LegalPage
      title="Politique de confidentialité"
      description="Quelles données sont traitées, pourquoi, combien de temps, et comment exercer vos droits sans compte."
      pageKey="confidentialite"
      details={details}
      toc={toc}
    >
      <h2 id="responsable">1. Responsable du traitement</h2>
      <p>
        Le responsable du traitement est l’éditeur du site,{" "}
        <LegalValue value={info?.publisher.name} pending={pending} />, demeurant{" "}
        <LegalValue value={info?.publisher.address} pending={pending} />. Pour toute question
        relative à vos données : <LegalEmail email={info?.privacy_email} pending={pending} />. Aucun
        délégué à la protection des données n’a été désigné, cette désignation n’étant pas
        obligatoire pour ce site.
      </p>

      <h2 id="principes">2. Nos principes</h2>
      <ul>
        <li>aucun compte, aucune adresse e-mail demandée pour proposer une idée ;</li>
        <li>aucun cookie, aucune publicité, aucun traceur publicitaire ;</li>
        <li>
          une seule ressource chargée par votre navigateur depuis un autre site : l’outil de mesure
          d’audience de l’éditeur, auto-hébergé, sans cookie, que vous pouvez refuser (voir{" "}
          <a href="#mesure-audience">la mesure d’audience</a>) ;
        </li>
        <li>aucune adresse IP enregistrée par l’application ni par la mesure d’audience ;</li>
        <li>des serveurs situés en France, administrés par l’éditeur.</li>
      </ul>

      <h2 id="traitements">3. Données traitées, finalités, bases légales et durées</h2>
      {processings.map((processing) => (
        <section key={processing.title} aria-label={processing.title}>
          <h3>{processing.title}</h3>
          <dl className="mb-4 grid gap-x-4 gap-y-1 sm:grid-cols-[10rem_minmax(0,1fr)]">
            <dt className="font-semibold">Données</dt>
            <dd>{processing.data}</dd>
            <dt className="font-semibold">Finalité</dt>
            <dd>{processing.purpose}</dd>
            <dt className="font-semibold">Base légale</dt>
            <dd>{processing.legalBasis}</dd>
            <dt className="font-semibold">Durée</dt>
            <dd>{processing.retention}</dd>
          </dl>
        </section>
      ))}

      <h3 id="conservation">Récapitulatif des durées de conservation</h3>
      <div className="mb-6 overflow-x-auto rounded-lg border">
        <table className="w-full min-w-[32rem] border-collapse text-sm">
          <caption className="sr-only">Durées de conservation des données</caption>
          <thead>
            <tr className="border-b bg-muted/40 text-left">
              <th scope="col" className="px-3 py-2 font-semibold">
                Donnée
              </th>
              <th scope="col" className="px-3 py-2 font-semibold">
                Conservation
              </th>
            </tr>
          </thead>
          <tbody>
            {retentionRows.map(([data, retention]) => (
              <tr key={data} className="border-b align-top last:border-b-0">
                <th scope="row" className="px-3 py-2 text-left font-normal">
                  {data}
                </th>
                <td className="px-3 py-2">{retention}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <h2 id="consignes">4. Consignes aux auteurs</h2>
      <p>
        <strong>Ne signez pas votre texte et ne nommez aucun agent ni particulier.</strong> Avant
        toute analyse par un service extérieur et avant publication, les adresses e-mail, numéros de
        téléphone, coordonnées bancaires, numéros de sécurité sociale, adresses postales et liens
        nominatifs sont remplacés automatiquement par « [donnée masquée] ». Un texte qui désigne un
        particulier identifiable est refusé. Le texte original affiché sur le site est cette version
        masquée.
      </p>

      <h2 id="ia">5. Intelligence artificielle</h2>
      <p>
        Le premier filtrage est réalisé par un modèle d’intelligence artificielle exécuté sur les
        serveurs de l’éditeur : votre texte ne quitte pas ces serveurs à cette étape.
      </p>
      <p>
        Le second avis en cas de doute, l’analyse approfondie des idées acceptées et l’examen des
        signalements sont confiés à <strong>Mistral AI</strong> (société française), en qualité de
        sous-traitant au sens de l’article 28 du RGPD, dans le cadre de son accord de traitement des
        données. L’option permettant à Mistral AI d’utiliser les données pour améliorer ses modèles
        est désactivée. Seul le texte, masqué de ses données personnelles, lui est transmis : jamais
        d’adresse IP, d’empreinte ni de lien de suivi.
      </p>
      <p>
        Le chiffrage s’appuie sur une recherche web effectuée par Mistral AI ; cette fonction peut
        faire appel à des prestataires situés hors de l’Union européenne (la liste des
        sous-traitants de Mistral AI est publiée sur son centre de confiance, trust.mistral.ai).
      </p>
      <p>
        Les décisions de filtrage sont prises automatiquement. Un refus n’a aucun effet juridique à
        votre égard : vous pouvez reformuler votre texte ou demander un réexamen humain depuis votre
        lien de suivi (voir les{" "}
        <Link to={`${paths.terms}#${RULES_RECOURSE_ANCHOR}`}>voies de recours</Link>). Les contenus
        générés par IA sont signalés comme tels sur le site et dans les exports, conformément à
        l’article 50 du règlement (UE) 2024/1689 sur l’intelligence artificielle.
      </p>

      <h2 id="destinataires">6. Destinataires</h2>
      <p>
        Les données sont accessibles à l’éditeur et, pour les besoins décrits ci-dessus, à Mistral
        AI. Les statistiques de mesure d’audience sont hébergées sur un serveur administré par
        l’éditeur (stats.methodinfo.fr) et ne sont communiquées à personne. Elles peuvent être
        communiquées à l’autorité judiciaire ou administrative sur réquisition ; la copie scellée
        d’un contenu escaladé peut être transmise aux autorités (plateforme PHAROS, autorité
        judiciaire). Elles ne sont jamais vendues ni utilisées à des fins publicitaires.
      </p>

      <h2 id="open-data">7. Données ouvertes</h2>
      <p>
        Les idées publiées et leurs analyses sont proposées en téléchargement (voir{" "}
        <Link to={paths.licence}>Licence et réutilisation</Link>). Ces fichiers ne contiennent que
        les contenus publiés et visibles, masqués de leurs données personnelles, sans identifiant
        technique ni horodatage précis (dates au jour près). Ils excluent les textes refusés,
        masqués, retirés par leur auteur ou en attente, les signalements et les coordonnées. Une
        copie déjà téléchargée par un tiers ne peut pas être retirée : la suppression d’un contenu
        s’applique au site et aux exports suivants (voir l’
        <Link to={`${paths.terms}#${RULES_LICENCE_ANCHOR}`}>article des règles sur la licence</Link>
        ).
      </p>

      <h2 id="stockage-local">8. Stockage dans votre navigateur</h2>
      <p>
        Le site n’utilise aucun cookie : aucun bandeau de consentement n’est donc nécessaire. Il
        enregistre localement, dans votre navigateur, uniquement :
      </p>
      <ul>
        <li>
          votre choix de thème clair ou sombre (clé <code>{THEME_STORAGE_KEY}</code>), si vous en
          faites un ;
        </li>
        <li>
          vos liens de suivi, uniquement si vous cochez « se souvenir sur cet appareil » (clé{" "}
          <code>{TRACKING_LINKS_STORAGE_KEY}</code>). Vous pouvez les oublier à tout moment depuis
          la page de suivi ou en effaçant les données du site dans votre navigateur ;
        </li>
        <li>
          votre refus de la mesure d’audience, si vous l’exprimez (clé{" "}
          <code>{AUDIENCE_OPT_OUT_KEY}</code>) ;
        </li>
        <li>
          pour l’administrateur seulement, sur le réseau local, son jeton d’accès, le temps de
          l’onglet (clé <code>{ADMIN_TOKEN_KEY}</code>).
        </li>
      </ul>
      <p>Ces informations ne sont jamais transmises au site ni à un tiers.</p>

      <h3 id="mesure-audience">Mesure d’audience</h3>
      <p>
        Le site mesure sa fréquentation avec Umami, un outil libre hébergé par l’éditeur, sans
        cookie ni identifiant conservé dans votre navigateur. Le fragment des adresses (qui contient
        votre lien de suivi) et leurs paramètres ne sont jamais transmis. Vous pouvez refuser cette
        mesure dans ce navigateur ; votre choix est enregistré localement et vaut aussi pour vos
        prochaines visites.
      </p>
      <AudienceOptOut />

      <h2 id="securite">9. Sécurité et sauvegardes</h2>
      <p>
        Les échanges sont chiffrés (HTTPS). L’administration du site n’est accessible que depuis le
        réseau local de l’éditeur et nécessite un jeton. Les sauvegardes de la base de données sont
        conservées 30 jours au plus.
      </p>

      <h2 id="droits">10. Vos droits</h2>
      <p>
        Vous disposez d’un droit d’accès, de rectification, d’effacement, de limitation et
        d’opposition sur les données qui vous concernent. Le droit à l’effacement ne s’applique pas
        à la copie scellée d’un contenu escaladé pendant l’année où elle est conservée (article 17,
        3, e du RGPD).
      </p>
      <ul>
        <li>
          <strong>Sans compte, votre lien de suivi est la preuve que vous êtes l’auteur</strong>{" "}
          d’un texte (article 11, 2 du RGPD). Depuis la page de suivi, vous pouvez consulter l’état
          de votre envoi, le supprimer ou demander un réexamen humain. Un texte non publié est
          effacé immédiatement ; une idée publiée est retirée immédiatement du site et des exports
          suivants, puis effacée 12 mois après le retrait (voir le{" "}
          <a href="#conservation">récapitulatif des durées</a>). Sans ce lien, le site ne peut pas
          savoir qu’un texte est le vôtre.
        </li>
        <li>
          Si un texte publié vous désigne, vous pouvez en demander le retrait sans lien de suivi en
          écrivant à <LegalEmail email={info?.privacy_email} pending={pending} /> avec l’adresse de
          la page concernée.
        </li>
        <li>
          Pour toute autre demande, écrivez à{" "}
          <LegalEmail email={info?.privacy_email} pending={pending} />. Une réponse vous est
          apportée dans un délai d’un mois.
        </li>
        <li>
          Vous pouvez introduire une réclamation auprès de la Commission nationale de l’informatique
          et des libertés (
          <a href={CNIL_COMPLAINT_URL} rel="noreferrer">
            cnil.fr
          </a>
          ).
        </li>
      </ul>
    </LegalPage>
  );
}
