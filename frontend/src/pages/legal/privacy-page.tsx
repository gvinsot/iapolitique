import { LegalPage, ToComplete } from "./legal-page";

export function PrivacyPage() {
  return (
    <LegalPage
      title="Confidentialité"
      description="Quelles données sont traitées, et surtout lesquelles ne le sont pas."
    >
      <h2>En résumé</h2>
      <p>
        iapolitique est conçu pour fonctionner <strong>sans donnée personnelle</strong> : pas de
        compte, pas d’adresse e-mail, pas de cookie, pas de mesure d’audience, aucun service tiers
        chargé par votre navigateur.
      </p>

      <h2>Ce qui est conservé</h2>
      <ul>
        <li>
          Le texte des idées, contributions et signalements que vous envoyez, ainsi que leur
          analyse. Évitez d’y inclure des informations permettant d’identifier quelqu’un.
        </li>
        <li>
          Une empreinte chiffrée (non réversible) de votre lien de suivi, jamais le lien lui-même.
        </li>
        <li>
          Les idées refusées sont supprimées 30 jours après le refus ; seules des statistiques
          agrégées, sans le texte, sont conservées.
        </li>
      </ul>

      <h2>Ce qui n’est pas conservé</h2>
      <ul>
        <li>Votre adresse IP, votre navigateur ou votre appareil.</li>
        <li>
          Pour limiter les abus, une empreinte temporaire de l’adresse IP, calculée avec une clé
          renouvelée chaque jour puis détruite, sert uniquement à compter les envois. L’adresse IP
          elle-même n’est jamais enregistrée.
        </li>
      </ul>

      <h2>Stockage dans votre navigateur</h2>
      <p>
        Seul votre choix de thème (clair ou sombre) est mémorisé, localement, dans votre navigateur.
        Il n’est jamais transmis.
      </p>

      <h2>Traitement par intelligence artificielle</h2>
      <p>
        Les textes envoyés sont analysés par un modèle hébergé par l’éditeur et par l’API de Mistral
        AI. Aucune donnée d’identification n’accompagne ces textes.
      </p>

      <h2>Responsable du traitement et droits</h2>
      <ToComplete>
        identité du responsable, base légale, durées de conservation détaillées, sous-traitants,
        exercice des droits et contact, réclamation auprès de la CNIL.
      </ToComplete>
    </LegalPage>
  );
}
