import { LegalPage, ToComplete } from "./legal-page";

export function LegalNoticePage() {
  return (
    <LegalPage
      title="Mentions légales"
      description="Informations sur l’éditeur et l’hébergeur du site."
    >
      <h2>Éditeur du site</h2>
      <ToComplete>
        nom ou dénomination de l’éditeur, statut juridique, adresse, moyen de contact, directeur ou
        directrice de la publication.
      </ToComplete>

      <h2>Hébergement</h2>
      <ToComplete>nom, adresse et moyen de contact de l’hébergeur.</ToComplete>

      <h2>Nature du site</h2>
      <p>
        iapolitique est un site indépendant et non officiel. Il n’a aucun lien avec l’État, une
        administration publique, une collectivité territoriale ou un parti politique.
      </p>
      <p>
        Les reformulations, analyses et chiffrages publiés sont générés automatiquement par
        intelligence artificielle à partir de propositions anonymes. Ils peuvent comporter des
        erreurs et ne constituent ni un avis officiel, ni un conseil.
      </p>

      <h2>Propriété intellectuelle et réutilisation</h2>
      <ToComplete>
        licence du code, des textes du site et des données exportées (décision ouverte).
      </ToComplete>

      <h2>Signaler un contenu</h2>
      <p>
        Chaque idée publiée dispose d’un bouton « Signaler » permettant de signaler une erreur de
        chiffrage, une source erronée ou un contenu problématique.
      </p>
      <ToComplete>contact pour les demandes relatives aux contenus illicites.</ToComplete>
    </LegalPage>
  );
}
