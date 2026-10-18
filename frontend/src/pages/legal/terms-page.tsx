import { LegalPage, ToComplete } from "./legal-page";

export function TermsPage() {
  return (
    <LegalPage
      title="Conditions générales d’utilisation"
      description="Les règles d’usage du site et de ses contenus."
    >
      <h2>Objet</h2>
      <p>
        iapolitique permet à toute personne de proposer anonymement des idées visant à réduire les
        dépenses ou à augmenter les recettes des administrations publiques françaises, et de
        consulter les analyses générées par intelligence artificielle.
      </p>

      <h2>Règles de dépôt</h2>
      <p>Les propositions doivent rester respectueuses et ne doivent pas :</p>
      <ul>
        <li>viser nommément une personne, un élu, un parti ou un groupe ;</li>
        <li>contenir de données personnelles ;</li>
        <li>tenter de manipuler les systèmes d’intelligence artificielle du site ;</li>
        <li>proposer des mesures contraires aux droits fondamentaux.</li>
      </ul>
      <p>
        Les propositions qui ne respectent pas ces règles sont refusées automatiquement, avec
        l’indication du motif.
      </p>

      <h2>Nature des analyses</h2>
      <p>
        Les analyses et chiffrages sont produits automatiquement et fournis à titre informatif. Ils
        peuvent être incomplets ou erronés et doivent être vérifiés à l’aide des sources citées.
      </p>

      <h2>Licence des contributions</h2>
      <ToComplete>
        licence sous laquelle les idées déposées et les analyses sont publiées.
      </ToComplete>

      <h2>Responsabilité, modification des conditions et droit applicable</h2>
      <ToComplete>
        limitation de responsabilité, modalités de modification des présentes conditions, droit
        applicable et juridiction compétente.
      </ToComplete>
    </LegalPage>
  );
}
