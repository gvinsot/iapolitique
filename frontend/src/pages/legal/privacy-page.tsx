import type { ReactNode } from "react";
import { Link } from "react-router";

import { ADMIN_TOKEN_KEY } from "@/api/admin-token";
import { paths } from "@/app/paths";
import { LegalEmail, LegalValue } from "@/components/legal/legal-details";
import { useLegalDetails } from "@/hooks/use-legal-info";
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
      <ul>
        <li>texte refusé : supprimé 30 jours après le refus, avec son lien de suivi ;</li>
        <li>texte publié : conservé pendant toute la durée de vie du site ;</li>
        <li>texte masqué après un signalement : supprimé 12 mois après le masquage ;</li>
        <li>texte supprimé par son auteur : effacé immédiatement.</li>
      </ul>
    ),
  },
  {
    title: "Lien de suivi",
    data: "Une empreinte cryptographique (SHA-256) du jeton de votre lien de suivi. Le lien lui-même n’est jamais enregistré par le site.",
    purpose:
      "Vous permettre de suivre votre envoi, de le supprimer ou de demander un réexamen, sans compte.",
    legalBasis: "Intérêt légitime (article 6, 1, f du RGPD).",
    retention: "Supprimée en même temps que le texte auquel elle se rapporte.",
  },
  {
    title: "Signalements",
    data: "Le motif, la précision que vous donnez et, si vous choisissez de les indiquer, vos nom et adresse e-mail.",
    purpose:
      "Examiner le signalement, corriger ou masquer le contenu en cause et, pour les contenus graves, les transmettre aux autorités.",
    legalBasis:
      "Intérêt légitime à modérer le site et respect des obligations de la loi pour la confiance dans l’économie numérique (article 6, 1, c et f du RGPD).",
    retention: "Supprimés 6 mois après leur traitement, coordonnées facultatives comprises.",
  },
  {
    title: "Demandes de réexamen humain",
    data: "Le message facultatif que vous joignez à votre demande.",
    purpose: "Faire réexaminer par un humain une décision prise automatiquement.",
    legalBasis: "Intérêt légitime (article 6, 1, f du RGPD).",
    retention: "Conservé avec le texte concerné et supprimé en même temps que lui.",
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
    title: "Demandes d’exercice de vos droits",
    data: "Votre message et l’adresse e-mail que vous utilisez pour nous écrire.",
    purpose: "Répondre à votre demande et en garder la preuve.",
    legalBasis: "Obligation légale (article 6, 1, c du RGPD).",
    retention: "1 an après la réponse.",
  },
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
        <li>aucun cookie, aucune publicité, aucune mesure d’audience ;</li>
        <li>aucune ressource chargée depuis un service tiers par votre navigateur ;</li>
        <li>aucune adresse IP enregistrée par l’application ;</li>
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
        AI. Elles peuvent être communiquées à l’autorité judiciaire ou administrative sur
        réquisition. Elles ne sont jamais vendues ni utilisées à des fins publicitaires.
      </p>

      <h2 id="open-data">7. Données ouvertes</h2>
      <p>
        Les idées publiées et leurs analyses sont proposées en téléchargement (voir{" "}
        <Link to={paths.licence}>Licence et réutilisation</Link>). Ces fichiers ne contiennent que
        les contenus publiés et visibles, masqués de leurs données personnelles, sans identifiant
        technique ni horodatage précis (dates au jour près). Ils excluent les textes refusés,
        masqués ou en attente, les signalements et les coordonnées. Une copie déjà téléchargée par
        un tiers ne peut pas être retirée : la suppression d’un contenu s’applique au site et aux
        exports suivants (voir l’
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
          pour l’administrateur seulement, sur le réseau local, son jeton d’accès, le temps de
          l’onglet (clé <code>{ADMIN_TOKEN_KEY}</code>).
        </li>
      </ul>
      <p>Ces informations ne sont jamais transmises au site ni à un tiers.</p>

      <h2 id="securite">9. Sécurité et sauvegardes</h2>
      <p>
        Les échanges sont chiffrés (HTTPS). L’administration du site n’est accessible que depuis le
        réseau local de l’éditeur et nécessite un jeton. Les sauvegardes de la base de données sont
        conservées 30 jours au plus.
      </p>

      <h2 id="droits">10. Vos droits</h2>
      <p>
        Vous disposez d’un droit d’accès, de rectification, d’effacement, de limitation et
        d’opposition sur les données qui vous concernent.
      </p>
      <ul>
        <li>
          <strong>Sans compte, votre lien de suivi est la preuve que vous êtes l’auteur</strong>{" "}
          d’un texte (article 11, 2 du RGPD). Depuis la page de suivi, vous pouvez consulter l’état
          de votre envoi, le supprimer immédiatement ou demander un réexamen humain. Sans ce lien,
          le site ne peut pas savoir qu’un texte est le vôtre.
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
