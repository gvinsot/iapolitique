import { Link } from "react-router";

import { paths } from "@/app/paths";
import { LegalEmail, LegalValue } from "@/components/legal/legal-details";
import { useLegalDetails } from "@/hooks/use-legal-info";
import { PHAROS_URL, SITE_DOMAIN } from "@/lib/legal";
import {
  RULES_ARTICLE_CRITERIA_ANCHOR,
  RULES_LICENCE_ANCHOR,
  RULES_RECOURSE_ANCHOR,
  RULES_SCORE_ANCHOR,
  ruleTargetLabels,
  screeningRules,
} from "@/lib/rules";

import { LegalPage, type TocEntry } from "./legal-page";

const toc: readonly TocEntry[] = [
  { anchor: "objet", label: "1. Objet" },
  { anchor: "nature", label: "2. Nature du site" },
  { anchor: "acceptation", label: "3. Accès et acceptation" },
  { anchor: RULES_ARTICLE_CRITERIA_ANCHOR, label: "4. Critères d’acceptation" },
  { anchor: "interdits", label: "5. Interdits" },
  { anchor: "moderation", label: "6. Modération algorithmique" },
  { anchor: "mesures", label: "7. Mesures et durées" },
  { anchor: RULES_RECOURSE_ANCHOR, label: "8. Voies de recours" },
  { anchor: RULES_SCORE_ANCHOR, label: "9. Classement et score" },
  { anchor: "analyses-ia", label: "10. Analyses générées par IA" },
  { anchor: RULES_LICENCE_ANCHOR, label: "11. Licence de votre contribution" },
  { anchor: "reutilisation", label: "12. Réutilisation des données" },
  { anchor: "signalement", label: "13. Signalement" },
  { anchor: "droit-de-reponse", label: "14. Droit de réponse" },
  { anchor: "responsabilite", label: "15. Responsabilité" },
  { anchor: "modification", label: "16. Modification des règles" },
  { anchor: "droit-applicable", label: "17. Droit applicable" },
];

/**
 * Rules of the site (CGU, AGENTS.md §7.5, docs/juridique/memo.md §1 and §3): numbered acceptance
 * and rejection criteria, algorithmic moderation, measures, recourse, score parameters and the
 * licence clause accepted with each publishable contribution.
 */
export function TermsPage() {
  const details = useLegalDetails();
  const { info, pending } = details;

  return (
    <LegalPage
      title="Règles du site"
      description="Conditions générales d’utilisation : ce qui peut être publié, comment la modération automatique fonctionne, comment contester une décision et sous quelle licence vos textes sont diffusés."
      pageKey="regles"
      details={details}
      toc={toc}
    >
      <p>
        Version des règles en vigueur :{" "}
        <strong>
          <LegalValue value={info?.cgu_version} pending={pending} />
        </strong>
        .
      </p>

      <h2 id="objet">Article 1 – Objet</h2>
      <p>
        {SITE_DOMAIN} permet à toute personne de proposer, sans compte et de façon anonyme, des
        idées pour réduire les dépenses ou augmenter les recettes des administrations publiques
        françaises (État, sécurité sociale, collectivités territoriales), de les améliorer ou d’y
        objecter, et de consulter les analyses et chiffrages générés par intelligence artificielle.
        Les présentes règles s’appliquent à tout visiteur et à tout envoi.
      </p>

      <h2 id="nature">Article 2 – Nature du site</h2>
      <p>
        Le site est édité à titre non professionnel par{" "}
        <LegalValue value={info?.publisher.name} pending={pending} /> (voir les{" "}
        <Link to={paths.legalNotice}>mentions légales</Link>). C’est un site citoyen indépendant et
        non officiel, sans lien avec l’État ni aucune administration, aucun parti politique ni
        Mistral AI. Publier une idée ne constitue ni une pétition, ni une consultation publique, ni
        une saisine d’une administration.
      </p>

      <h2 id="acceptation">Article 3 – Accès et acceptation</h2>
      <p>
        La consultation du site est libre et gratuite. Pour envoyer une idée, une amélioration ou
        une objection, vous devez cocher deux cases, jamais pré-cochées : l’acceptation de la
        licence de l’article 11, et la mention « je comprends que mon texte sera publié s’il est
        accepté ». La version des règles acceptée et la date d’acceptation (au jour près) sont
        enregistrées avec votre texte, sans aucune donnée vous identifiant.
      </p>
      <p>
        Chaque envoi donne lieu à un lien de suivi personnel, affiché une seule fois. Il permet de
        connaître le devenir de l’envoi, de le supprimer et de demander un réexamen humain. Il ne
        peut pas être retrouvé s’il est perdu.
      </p>

      <h2 id={RULES_ARTICLE_CRITERIA_ANCHOR}>Article 4 – Critères d’acceptation et de rejet</h2>
      <p>
        Un texte est accepté s’il respecte tous les critères ci-dessous qui s’appliquent à lui. Ils
        sont vérifiés dans cet ordre ; le premier critère non respecté est le motif du refus, qui
        vous est indiqué immédiatement avec une invitation à reformuler.
      </p>
      <ol className="list-none! pl-0!">
        {screeningRules.map((rule) => (
          <li key={rule.key} id={rule.anchor} className="mb-4">
            <h3 className="mt-0!">
              {rule.number}. {rule.title}
            </h3>
            <p className="mb-1!">{rule.requirement}</p>
            <p className="text-sm text-muted-foreground">
              S’applique aux {rule.appliesTo.map((target) => ruleTargetLabels[target]).join(", ")}.
            </p>
          </li>
        ))}
      </ol>

      <h2 id="interdits">Article 5 – Interdits</h2>
      <p>Il est interdit, dans tout texte envoyé :</p>
      <ul>
        <li>
          de nommer ou de rendre identifiable une personne physique, y compris un agent public (ne
          signez pas non plus votre texte) ;
        </li>
        <li>d’injurier, de diffamer, de menacer ou d’inciter à la haine ou à la violence ;</li>
        <li>de promouvoir ou de dénigrer un candidat, un élu ou un parti politique ;</li>
        <li>
          de reproduire des contenus de tiers au-delà de courtes citations dont la source est
          indiquée ;
        </li>
        <li>de tenter de manipuler les systèmes d’intelligence artificielle du site ;</li>
        <li>d’envoyer des messages en masse ou de manière automatisée.</li>
      </ul>

      <h2 id="moderation">Article 6 – Modération algorithmique</h2>
      <p>
        La modération est <strong>automatique</strong> et ne comporte pas de relecture humaine
        systématique :
      </p>
      <ul>
        <li>
          chaque texte est évalué par un modèle de décision (d3-mini) exécuté sur les serveurs de
          l’éditeur, qui donne pour chaque critère de l’article 4 une probabilité comparée à deux
          seuils publics : en dessous du premier, le texte est refusé ; au-dessus du second, il est
          accepté ;
        </li>
        <li>
          entre les deux seuils, un second avis est demandé à un modèle de Mistral AI avec la même
          grille ; le texte reste alors « en cours de vérification » ;
        </li>
        <li>
          une idée acceptée est comparée aux idées déjà publiées : si elle est quasi identique à
          l’une d’elles, elle y est rattachée comme contribution plutôt que publiée une seconde fois
          ;
        </li>
        <li>
          les signalements sont examinés par un modèle de Mistral AI, qui conclut au maintien de
          l’idée, à la mise à jour de son analyse ou à son masquage ;
        </li>
        <li>
          les contenus graves (diffamation, menace, haine, terrorisme, atteinte aux mineurs) sont
          masqués immédiatement et placés dans une file d’examen humain, avec transmission si
          nécessaire à la plateforme officielle PHAROS (
          <a href={PHAROS_URL} rel="noreferrer">
            internet-signalement.gouv.fr
          </a>
          ) dans un délai visé de 24 heures.
        </li>
      </ul>
      <p>
        Les consignes données aux modèles, la grille de filtrage et ses seuils sont publiés sur la
        page <Link to={paths.methodology}>Méthodologie</Link>. L’éditeur peut à tout moment masquer
        ou republier une idée et relancer une analyse.
      </p>

      <h2 id="mesures">Article 7 – Mesures et durées</h2>
      <ul>
        <li>
          <strong>Refus</strong> : le texte n’est pas publié ; il est supprimé 30 jours après le
          refus. Vous pouvez envoyer une version reformulée à tout moment.
        </li>
        <li>
          <strong>Masquage</strong> : l’idée est retirée du site et des exports suivants ; son texte
          est supprimé 12 mois après le masquage, sauf republication.
        </li>
        <li>
          <strong>Mise à jour</strong> : l’analyse d’une idée est refaite à la suite d’une
          contribution acceptée, d’un doublon rattaché ou d’un signalement fondé.
        </li>
        <li>
          <strong>Rattachement</strong> : un doublon devient une contribution de l’idée existante,
          dont le compteur « proposée N fois » augmente.
        </li>
        <li>
          <strong>Suppression par l’auteur</strong> : depuis le lien de suivi, effacement immédiat
          du site et retrait des exports suivants.
        </li>
      </ul>
      <p>
        Toute mesure est indiquée sur le lien de suivi de l’auteur, avec son motif, la règle en
        cause, la mention « décision prise automatiquement par une IA », la date de suppression du
        texte le cas échéant et les voies de recours.
      </p>

      <h2 id={RULES_RECOURSE_ANCHOR}>Article 8 – Voies de recours</h2>
      <ol>
        <li>
          <strong>Réexamen humain</strong> : depuis votre lien de suivi, le bouton « Demander un
          réexamen humain » place votre texte, avec un message facultatif, dans la file d’examen de
          l’éditeur.
        </li>
        <li>
          <strong>Contact</strong> : vous pouvez écrire à{" "}
          <LegalEmail email={info?.contact_email} pending={pending} />.
        </li>
        <li>
          <strong>Juge</strong> : vous pouvez saisir la juridiction française compétente.
        </li>
      </ol>
      <p>Un refus n’a aucun effet juridique : il ne vous empêche pas de reformuler votre texte.</p>

      <h2 id={RULES_SCORE_ANCHOR}>Article 9 – Classement et score</h2>
      <p>
        Les idées publiées sont classées par défaut selon un{" "}
        <strong>score calculé par algorithme</strong>, à partir des estimations de l’analyse générée
        par IA. Ce score n’est <strong>ni un sondage, ni un vote, ni une mesure d’opinion</strong>,
        et aucun score n’est ajusté à la main. Ses paramètres sont :
      </p>
      <ul>
        <li>
          le <strong>gain net annuel</strong> : gain annuel médian estimé, diminué du coût médian de
          mise en œuvre amorti sur 5 ans ;
        </li>
        <li>
          la <strong>faisabilité</strong>, notée de 1 à 5 et divisée par 5 ;
        </li>
        <li>
          la <strong>confiance</strong> dans les données disponibles : 0,5 si elle est faible, 0,75
          si elle est moyenne, 1 si elle est élevée ;
        </li>
        <li>
          le <strong>délai</strong> avant les premiers effets : 1 / (1 + délai en mois / 24).
        </li>
      </ul>
      <p>
        Score = gain net annuel × faisabilité × confiance × délai. Les composantes sont affichées
        sur chaque idée ; la formule en vigueur et sa version sont publiées sur la page{" "}
        <Link to={paths.methodology}>Méthodologie</Link>. Vous pouvez aussi trier les idées par
        gain, faisabilité, délai ou date.
      </p>

      <h2 id="analyses-ia">Article 10 – Analyses générées par IA</h2>
      <p>
        Les reformulations, propositions améliorées, avantages, inconvénients, impacts et chiffrages
        sont générés automatiquement par intelligence artificielle (Mistral AI), sans relecture
        humaine. Ils peuvent contenir des erreurs, ne sont pas des chiffrages officiels et ne
        constituent ni un conseil ni une position officielle. Ils sont signalés comme tels sur le
        site, et le texte original de l’internaute reste affiché séparément.
      </p>

      <h2 id={RULES_LICENCE_ANCHOR}>Article 11 – Licence de votre contribution</h2>
      <ol>
        <li>
          Vous déclarez être l’auteur du texte soumis et n’y inclure aucun contenu de tiers au-delà
          de courtes citations dont vous indiquez la source.
        </li>
        <li>
          Vous concédez à l’éditeur d’{SITE_DOMAIN}, à titre gratuit et non exclusif, pour le monde
          entier et pour toute la durée légale des droits d’auteur, les droits suivants sur votre
          texte : (a) reproduction, sur tout support numérique ; (b) représentation et communication
          au public sur le site {SITE_DOMAIN} et dans ses jeux de données ; (c) adaptation, à savoir
          analyse, résumé, reformulation et traduction par des systèmes d’intelligence artificielle,
          et masquage des données personnelles qu’il contiendrait ; (d) intégration dans des jeux de
          données ouverts.
        </li>
        <li>
          Vous autorisez l’éditeur à sous-licencier ces droits au public sous Licence Ouverte 2.0
          (Etalab) ou Creative Commons BY 4.0.
        </li>
        <li>
          Votre texte original reste affiché, sous réserve du seul masquage des données
          personnelles, à côté de toute reformulation, laquelle est signalée comme générée par IA.
        </li>
        <li>
          Vous acceptez une publication sans mention de votre nom, la paternité étant indiquée sous
          la forme « Contribution anonyme – {SITE_DOMAIN} ».
        </li>
        <li>
          Vous êtes informé qu’une contribution publiée puis téléchargée par des tiers ne peut plus
          être retirée de leurs copies. Son retrait du site s’applique au site et aux exports
          ultérieurs.
        </li>
      </ol>

      <h2 id="reutilisation">Article 12 – Réutilisation des données</h2>
      <p>
        Les idées publiées et leurs analyses peuvent être téléchargées et réutilisées librement sous
        Licence Ouverte 2.0, dans la mesure où des droits existent, en citant la source et la date
        de mise à jour. Voir la page <Link to={paths.licence}>Licence et réutilisation</Link>.
      </p>

      <h2 id="signalement">Article 13 – Signalement</h2>
      <p>
        Chaque idée publiée dispose d’un bouton « Signaler » : choisissez un motif (erreur de
        chiffrage, source erronée ou absente, contenu problématique, doublon, autre), précisez
        éventuellement le problème et, si vous le souhaitez, indiquez vos nom et adresse e-mail.
        Vous devez déclarer que votre signalement est fait de bonne foi. Un accusé de réception et
        la suite donnée sont consultables sur le lien de suivi du signalement. Le signalement est
        traité automatiquement (article 6).
      </p>
      <p>
        Présenter sciemment un contenu comme illicite dans le but d’en obtenir le retrait ou d’en
        faire cesser la diffusion est puni d’un an d’emprisonnement et de 15 000 € d’amende (article
        6, VII de la loi n° 2004-575 du 21 juin 2004).
      </p>

      <h2 id="droit-de-reponse">Article 14 – Droit de réponse</h2>
      <p>
        Toute personne nommée ou désignée sur le site peut exercer un droit de réponse dans les
        trois mois suivant la mise en ligne, en écrivant au directeur de la publication à{" "}
        <LegalEmail email={info?.contact_email} pending={pending} />. La réponse est publiée sous
        l’idée concernée dans les trois jours suivant la réception de la demande.
      </p>

      <h2 id="responsabilite">Article 15 – Responsabilité</h2>
      <p>
        Vous êtes responsable des textes que vous envoyez. L’éditeur, directeur de la publication,
        répond des contenus publiés sur le site dans les conditions prévues par la loi. Il s’efforce
        d’assurer la disponibilité du site sans pouvoir la garantir, et peut suspendre le dépôt de
        textes, notamment pour maintenance ou en cas d’abus.
      </p>

      <h2 id="modification">Article 16 – Modification des règles</h2>
      <p>
        Ces règles peuvent évoluer ; chaque version porte un numéro et une date, affichés en haut de
        cette page. Un texte reste régi par la version qui était en vigueur lors de son envoi.
      </p>

      <h2 id="droit-applicable">Article 17 – Droit applicable</h2>
      <p>
        Les présentes règles sont régies par le droit français. Pour toute question :{" "}
        <LegalEmail email={info?.contact_email} pending={pending} />.
      </p>
    </LegalPage>
  );
}
