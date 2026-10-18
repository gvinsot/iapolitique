# Mémo juridique (octobre 2026)

> Synthèse de recherches automatisées vérifiées par des agents contradicteurs. Ce n'est pas un avis juridique : les points de la section 5 restent à faire valider par un juriste.
>
> **Décisions prises le 2026-10-18** : Licence Ouverte 2.0 (Etalab) pour les exports ; éditeur personne physique avec identité publiée (option A, coordonnées lues depuis la configuration, jamais committées) ; aucune IP en clair (journaux par défaut de la section 4).


*État du droit au 18/10/2026. Ce mémo s'adresse au propriétaire, développeur et non juriste. Les points qui restent incertains sont signalés comme tels et repris en section 5.*

---

## 1. Licence recommandée pour les exports

### Le choix : Licence Ouverte / Open Licence 2.0 (Etalab)

- Identifiant SPDX : `etalab-2.0`. Identifiant data.gouv.fr : `lov2`.
- Elle est compatible avec CC BY 4.0, ce que la licence prévoit elle-même.
- Texte : https://raw.githubusercontent.com/etalab/licence-ouverte/master/LO.md (fiche : https://www.data.gouv.fr/pages/legal/licences/etalab-2.0)

### Pourquoi cette licence

1. **Un particulier peut l'utiliser.** Le « Concédant » y est défini comme « toute personne ». Elle est régie par le droit français et concédée pour le monde entier, sans limite de durée.
2. **Sa seule condition est de citer la source et la date de dernière mise à jour.** C'est adapté à des analyses qui changent à chaque ré-analyse. La licence interdit aussi d'induire en erreur sur le contenu, la source ou la date, ce qui gêne la présentation d'une analyse IA comme une position officielle.
3. **Les administrations la connaissent.** Elle figure sur la liste réglementaire des licences qu'elles appliquent à leurs propres données (art. D323-2-1 CRPA, https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000034504991).
   - Nuance : cette liste ne s'impose pas à vous. Une administration qui reprend des données de tiers applique la licence du tiers (art. L321-2 CRPA).
   - L'argument reste donc pratique (moins de friction), pas juridique.

### Les autres licences, écartées

- **ODbL et CC BY-SA** : le partage à l'identique freine la reprise par les administrations, la presse et les partis de tous bords. Vous ne pourriez pas non plus le faire respecter vous-même. Il n'apporte rien à un projet apolitique dont le but est la diffusion maximale.
- **CC0** : défendable, mais elle supprime l'obligation de citer la source et la date. Cette obligation garde une utilité pour les textes des contributeurs.
- **CC BY 4.0** : équivalente à la LO 2.0. Elle ne serait préférable que pour viser surtout un public international. Une double licence LO 2.0 / CC BY 4.0 reste possible.

### Limites à connaître (et à écrire honnêtement dans la notice)

- **Les champs générés par IA sont probablement du domaine public.** Le rapport du CSPLA du 16/07/2026 (avis consultatif, pas du droit positif) le dit pour les productions « synthétiques », générées sans intervention humaine significative : https://www.culture.gouv.fr/nous-connaitre/organisation-du-ministere/conseil-superieur-de-la-propriete-litteraire-et-artistique-cspla/travaux-et-publications-du-cspla/missions-du-cspla/rapport-de-mission-sur-le-statut-des-productions-de-l-intelligence-artificielle
  - La condition de paternité ne peut donc pas être imposée sur ces champs.
  - La traçabilité doit reposer sur les métadonnées de l'export : modèle, date, version d'analyse, URL canonique.
- **Le droit sui generis sur la base n'est pas acquis** (art. L341-1 CPI, https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000006279245).
  - Selon la CJUE (BHB, C-203/02, https://eur-lex.europa.eu/legal-content/FR/TXT/?uri=CELEX:62002CJ0203), l'argent dépensé pour *créer* les données ne compte pas. Les appels à l'API Mistral sont donc exclus.
  - Il faut écrire que la licence vaut « dans la mesure où des droits existent ».
- **Les contenus de tiers cités (titres, extraits) sont exclus de la licence.**
  - L'exception de courte citation (art. L122-5 3° a CPI, https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000049416467) suppose une citation incorporée dans une « œuvre ». Une analyse générée par IA n'en est probablement pas une, donc l'exception est fragile ici.
  - Il faut se limiter à des références bibliographiques : titre, URL, éditeur, date.
- **Données personnelles.** La LO 2.0 oblige le Concédant à signaler « par tous moyens » la présence de données personnelles. Il faut donc caviarder avant d'exporter et le dire dans la notice.
- **Irrévocabilité.** Une idée publiée puis téléchargée ne peut pas être retirée des copies déjà diffusées. Masquer une idée n'agit que sur le site et sur les exports suivants.

### Montage avec les contributeurs : qui accorde la licence à qui

L'auteur anonyme ne doit pas être lui-même le « Concédant » sous LO 2.0, car la licence impose de citer « a minima le nom du Concédant ». Le montage retenu est le suivant :

- le contributeur accorde à l'éditeur une licence gratuite, non exclusive, mondiale, pour la durée des droits ;
- cette licence inclut le **droit de sous-licencier** les textes sous LO 2.0 ou CC BY 4.0 ;
- l'éditeur, « iapolitique.fr », devient ainsi le seul Concédant.

Ce montage est cohérent avec l'art. L113-6 CPI : l'éditeur représente les auteurs anonymes, sans devenir propriétaire de leurs droits (https://www.legifrance.gouv.fr/codes/section_lc/LEGITEXT000006069414/LEGISCTA000006161636/).

Trois exigences encadrent cette licence :
- **L'acceptation doit être écrite** (art. L131-2 al. 2 CPI). Une acceptation électronique horodatée vaut écrit (C. civ. art. 1366 et 1127-1 et suivants).
- **Chaque droit doit être énuméré et son domaine délimité** (art. L131-3 CPI).
- **Le droit moral ne se cède pas.** Le consentement à la reformulation par IA ne protège que si le texte original reste affiché à côté.

### Mention d'attribution à proposer aux réutilisateurs

> Source : iapolitique.fr – Propositions citoyennes et analyses générées par IA (Mistral AI), mise à jour du AAAA-MM-JJ – Licence Ouverte 2.0

À ajouter en simple demande, puisque la LO 2.0 ne permet pas d'ajouter de conditions :

> Analyses produites automatiquement, non relues par un humain ; vérifiez les sources citées.

Pour les textes des internautes, la paternité s'écrit : « Contribution anonyme – iapolitique.fr ».

### Clause à faire accepter (CGU, avec case non pré-cochée sur chaque formulaire de contribution publiable)

> **Article X – Licence de votre contribution.**
> 1. Vous déclarez être l'auteur du texte soumis et n'y inclure aucun contenu de tiers au-delà de courtes citations dont vous indiquez la source.
> 2. Vous concédez à l'éditeur d'iapolitique.fr, à titre gratuit et non exclusif, pour le monde entier et pour toute la durée légale des droits d'auteur, les droits suivants sur votre texte : (a) reproduction, sur tout support numérique ; (b) représentation et communication au public sur le site iapolitique.fr et dans ses jeux de données ; (c) adaptation, à savoir analyse, résumé, reformulation et traduction par des systèmes d'intelligence artificielle, et masquage des données personnelles qu'il contiendrait ; (d) intégration dans des jeux de données ouverts.
> 3. Vous autorisez l'éditeur à sous-licencier ces droits au public sous Licence Ouverte 2.0 (Etalab) ou Creative Commons BY 4.0.
> 4. Votre texte original reste affiché, sous réserve du seul masquage des données personnelles, à côté de toute reformulation, laquelle est signalée comme générée par IA.
> 5. Vous acceptez une publication sans mention de votre nom, la paternité étant indiquée sous la forme « Contribution anonyme – iapolitique.fr ».
> 6. Vous êtes informé qu'une contribution publiée puis téléchargée par des tiers ne peut plus être retirée de leurs copies. Son retrait du site s'applique au site et aux exports ultérieurs.

Il faut stocker avec chaque contribution un `cgu_version` et une date d'acceptation, sans aucune donnée personnelle.

---

## 2. Mentions légales

### Ce que la loi exige de vous

Le texte applicable est l'art. 1-1 de la LCEN, créé par la loi SREN et en vigueur depuis le 23/05/2024 : https://www.legifrance.gouv.fr/loda/id/JORFTEXT000000801164. Les modèles qui citent encore « art. 6 III LCEN » sont périmés.

Une personne physique qui édite un site doit publier, dans un standard ouvert (une page HTML suffit) :
- ses **nom, prénoms, domicile et numéro de téléphone** ;
- le **nom du directeur de la publication**. C'est obligatoirement vous (art. 93-2 de la loi n° 82-652, https://www.legifrance.gouv.fr/loda/article_lc/LEGIARTI000033971722) ;
- le **nom, l'adresse et le téléphone de l'hébergeur**.

Aucun e-mail n'est exigé par ce texte. En revanche, un point de contact est nécessaire en pratique (voir section 3).

L'absence de ces mentions est punie d'1 an d'emprisonnement et 75 000 € d'amende (art. 1-2 LCEN). De plus, l'art. 6-1 LCEN permet de bloquer administrativement un site *sans demande préalable* lorsque l'éditeur n'a pas publié ses mentions.

### La question de l'anonymat

L'art. 1-1 II permet à un éditeur **non professionnel** de ne publier que le nom (ou la raison sociale) et l'adresse de son **hébergeur**, à condition de lui avoir communiqué son identité. Le téléphone de l'hébergeur n'est alors pas exigé. Vous êtes aujourd'hui éditeur non professionnel : pas de société, pas de revenus, pas de publicité.

**En auto-hébergement, cette option ne sert à rien.** On ne se fournit pas un service d'hébergement à soi-même (art. 6 I 2° LCEN, qui renvoie à l'art. 3 g) iii) du DSA). Il n'y a donc pas de tiers à citer.

Points annexes :
- Il ne faut pas citer votre FAI ni Cloudflare comme « hébergeur ». Un FAI fait du transport, un CDN du cache.
- Le masquage Whois de l'Afnic (automatique pour un particulier) ne dispense de rien. L'Afnic peut lever l'anonymat sur réquisition judiciaire (https://www.afnic.fr/en/domain-names-and-support/resolve-a-dispute/data-disclosure-request/).

### Vos options

| Option | Ce qui devient public | Avantages | Inconvénients |
|---|---|---|---|
| **A. Identité publiée** | Nom, domicile, téléphone. Une ligne VoIP ou une seconde SIM dédiée convient pour le téléphone. | Simple, gratuit, aucun changement d'architecture. | Votre adresse personnelle est visible sur un site à thème politique (risque de harcèlement). |
| **B. Hébergeur tiers + anonymat (art. 1-1 II)** | Nom et adresse de l'hébergeur. | Votre identité n'est pas publiée. | Le contenu public (frontend, idées publiées, point de soumission) doit être **réellement stocké** chez un tiers (VPS en France ou dans l'UE). Un simple reverse proxy vers la maison ne suffit pas, car c'est du « caching ». Il faut une architecture hybride (WireGuard vers le homelab pour l'IA et la base complète), on paie un coût, on ajoute un point de défaillance et un second lieu de traitement à documenter. **Limite** : la politique de confidentialité doit de toute façon donner l'« identité » du responsable de traitement (art. 13(1)(a) RGPD), voir section 5. |
| **C. Tout chez un hébergeur tiers** | Comme B. | Même avantage que B, plus simple. | Vous abandonnez le homelab pour la partie publique. |
| **D. Association loi 1901** | Dénomination, siège (une adresse de domiciliation est possible), téléphone. Le nom du président, directeur de la publication, reste public, mais **pas son domicile**. | Souvent le meilleur compromis. Résout aussi la question du responsable de traitement au sens du RGPD et, peut-être, l'accès à l'API Mistral (voir section 5). Publication au JOAFE gratuite depuis 2020. | Il faut au moins 2 membres, des statuts et un minimum de vie associative. L'association devient éditeur, Concédant de la licence et responsable de traitement : tous les textes sont à adapter. |

**Recommandation** : choisir **D** si vous voulez protéger votre domicile, sinon **A**. Choisir B seulement si vous voulez rester seul et anonyme et acceptez l'architecture hybride ainsi que le risque RGPD décrit en section 5.

### Modèle `/mentions-legales`

Les variantes s'excluent l'une l'autre. Supprimez celles qui ne s'appliquent pas.

```
# Mentions légales
Dernière mise à jour : [JJ/MM/AAAA]

## 1. Éditeur
[A] iapolitique.fr est édité à titre non professionnel par [Prénom NOM], demeurant [adresse du domicile], téléphone : [numéro], courriel : contact@iapolitique.fr.
[B] iapolitique.fr est édité à titre non professionnel par une personne physique qui, conformément au II de l'article 1-1 de la loi n° 2004-575 du 21 juin 2004 (LCEN), a communiqué ses éléments d'identification à l'hébergeur mentionné ci-dessous. Courriel : contact@iapolitique.fr.
[D] iapolitique.fr est édité par l'association [Nom], association loi 1901 déclarée le [date] (RNA [n°]), siège : [adresse], téléphone : [numéro], courriel : contact@iapolitique.fr.

## 2. Directeur de la publication
[A] [Prénom NOM].  [B] L'éditeur, dont l'identité est connue de l'hébergeur.  [D] [Prénom NOM], président(e) de l'association.

## 3. Hébergement
[A/D si auto-hébergé] Site auto-hébergé par l'éditeur sur ses propres serveurs situés en France (coordonnées ci-dessus).
[B/C] [Raison sociale], [adresse du siège][, téléphone].
Analyse par IA : Mistral AI (sous-traitant, voir Politique de confidentialité). Le modèle de modération est exécuté sur les serveurs de l'éditeur.

## 4. Site non officiel
iapolitique.fr est une initiative citoyenne indépendante, non commerciale et apartisane. Il n'est ni édité, ni financé, ni validé par l'État, une administration, une collectivité territoriale, un organisme de sécurité sociale, un parti politique ou Mistral AI. La publication d'une idée ne constitue ni une pétition, ni une consultation publique, ni une saisine d'une administration. Le score de classement est calculé par un algorithme ; ce n'est ni un sondage ni une mesure d'opinion.

## 5. Contenus générés par intelligence artificielle
Les reformulations, propositions améliorées, avantages et inconvénients, impacts, estimations d'économies et classements sont générés automatiquement par IA, sans relecture humaine. Ils peuvent contenir des erreurs. Les estimations sont indicatives, ne sont pas des chiffrages officiels et doivent être vérifiées à partir des sources citées.

## 6. Contributions des internautes
Le texte original de chaque idée est publié dans un espace « Contribution d'un internaute », après filtrage automatisé et masquage des données personnelles. Règles de publication : [lien /regles].

## 7. Signaler un contenu – point de contact
Formulaire « Signaler » sur chaque idée, ou contact@iapolitique.fr (lu par un humain ; point de contact unique pour les autorités et les utilisateurs, règlement (UE) 2022/2065 art. 11 et 12 ; langue : français). Indiquez le motif, l'URL, et facultativement vos nom et courriel. Le premier traitement est automatisé. Présenter sciemment un contenu comme illicite pour obtenir son retrait est puni d'un an d'emprisonnement et de 15 000 € d'amende (art. 6 VII LCEN).

## 8. Droit de réponse
Toute personne nommée ou désignée sur le site dispose d'un droit de réponse (art. 1-1 III LCEN), à exercer dans les trois mois suivant la mise en ligne, par demande adressée au directeur de la publication à contact@iapolitique.fr [B : ou à l'hébergeur, qui la transmet], dans les formes du décret n° 2007-1527 du 24 octobre 2007.

## 9. Propriété intellectuelle et réutilisation
Code source : [licence / lien]. Données exportées : Licence Ouverte 2.0, dans la mesure où des droits existent ; contenus de tiers cités exclus. Voir [page Licence et réutilisation].

## 10. Données personnelles
Voir la [Politique de confidentialité].

## 11. Droit applicable
Droit français.
```

---

## 3. Autres pages et obligations à prévoir

| Élément | Statut | Fondement |
|---|---|---|
| Mentions légales | **OBLIGATOIRE** | LCEN art. 1-1 et 1-2 |
| Politique de confidentialité | **OBLIGATOIRE** | RGPD art. 13 et 14. L'exemption « domestique » ne joue pas pour un site public (CJUE Lindqvist C-101/01, https://curia.europa.eu/juris/liste.jsf?num=C-101/01) |
| Registre des traitements (document interne) | **OBLIGATOIRE** | RGPD art. 30(5) : le traitement n'est pas occasionnel. Modèle CNIL : https://www.cnil.fr/fr/RGPD-le-registre-des-activites-de-traitement |
| Registre interne des violations de données | **OBLIGATOIRE** | RGPD art. 33(5). Notification à la CNIL sous 72 h seulement si la violation présente un risque |
| Clause de licence dans les CGU, acceptation horodatée | **OBLIGATOIRE** pour pouvoir exporter les textes des contributeurs | CPI L131-2, L131-3, L122-7-1 |
| Procédure de droit de réponse (insertion sous 3 jours) | **OBLIGATOIRE** | LCEN art. 1-1 III (amende de 3 750 €) ; décret n° 2007-1527 |
| Contact humain surveillé, capable de traiter un retrait administratif sous 24 h | **OBLIGATOIRE** | LCEN art. 6-1 : vise aussi les éditeurs, même hors champ du DSA |
| Étiquette IA visible sur chaque analyse | **RECOMMANDÉ fortement** (obligatoire si vous êtes « déployeur ») | AI Act art. 50(4) et 50(5) ; CGV Mistral art. 3.2 (obligation contractuelle) |
| Marquage IA lisible par machine (HTML + exports) | **OBLIGATOIRE probable** dès le lancement | AI Act art. 50(2), si vous êtes « fournisseur » |
| CGU / règles de publication et de modération | **RECOMMANDÉ** (obligatoire si le DSA s'applique) | DSA art. 14 |
| Formulaire de signalement + accusé de réception | **RECOMMANDÉ** (obligatoire si le DSA s'applique) | DSA art. 16 ; LCEN art. 6 VII |
| Exposé des motifs sur le lien de suivi | **RECOMMANDÉ** | DSA art. 17(3). L'obligation tombe faute de coordonnées connues (art. 17(2)) |
| Escalade des contenus graves vers PHAROS | **RECOMMANDÉ** (obligatoire si hébergeur : 1 an et 250 000 €) | LCEN art. 6 IV A et IV C ; DSA art. 18 |
| Avertissement « site non officiel », sans Marianne ni DSFR | **RECOMMANDÉ** (ne pas imiter l'État = obligatoire de fait) | Code pénal art. 433-13 (https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000006418574) |
| Page « Comment fonctionnent la modération et le score » | **RECOMMANDÉ** | DSA art. 14 (et art. 27 par prudence) ; RGPD art. 22, non applicable mais bonne pratique |
| AIPD courte | **RECOMMANDÉ** | RGPD art. 35 : technologie innovante et possibles opinions politiques |
| Accessibilité WCAG 2.1 AA | **OPTIONNEL** (le RGAA ne s'impose pas à un particulier) | https://accessibilite.numerique.gouv.fr/obligations/champ-application/ ; AI Act art. 50(5) pour les étiquettes |
| Bandeau cookies | **NON REQUIS**, si ALTCHA reste auto-hébergé sans cookie, sans aucune ressource tierce, et si le seul stockage local est strictement nécessaire et décrit | Loi Informatique et Libertés art. 82, https://www.cnil.fr/fr/cookies-et-autres-traceurs/regles/cookies/que-dit-la-loi |

### Le DSA s'applique-t-il ? Question non tranchée

- **Pour l'exclusion** : le DSA ne vise que les services « normalement fournis contre rémunération ». Sans publicité, dons ni contrepartie, l'absence d'activité économique est l'argument le plus solide (considérant 18 de la directive 2000/31, https://eur-lex.europa.eu/legal-content/FR/TXT/?uri=CELEX:32000L0031).
- **Pour l'application** : l'arrêt McFadden (C-484/14) apprécie la rémunération selon le type de service, pas selon l'opérateur, ce qui pousse dans l'autre sens.
- **Conséquence pratique** : appliquer un « DSA léger » (contact, signalement, motifs, règles), qui ne coûte presque rien.
- **À éviter** : les dons et la publicité, qui feraient aussi perdre le statut « non professionnel » de l'art. 1-1 II.

### Votre statut : raisonner comme un éditeur

- **Les analyses IA sont votre propre contenu.** Elles sont fixées avant leur mise en ligne. En diffamation ou en injure, vous en répondez comme auteur principal en tant que directeur de la publication (art. 93-3, https://www.legifrance.gouv.fr/loda/article_lc/LEGIARTI000020740559).
- **Pour les textes des internautes, le régime atténué est fragile.**
  - Un arrêt de la CJUE (16/06/2026, C-188/24 et C-190/24, communiqué : https://curia.europa.eu/site/upload/docs/application/pdf/2026-06/cp260087fr.pdf) aurait jugé qu'un algorithme qui décide de la diffusion caractérise un « contrôle » faisant perdre l'exemption d'hébergeur. Ce point est tiré des commentaires : ni la recherche ni la vérification n'ont lu l'arrêt intégral.
  - En sens inverse : l'art. 7 du DSA (« bon samaritain »), et l'art. 93-3 al. 5, qui prévoit deux voies d'exonération alternatives (absence de connaissance avant la mise en ligne, OU retrait prompt). Voir aussi la décision 2011-164 QPC : https://www.conseil-constitutionnel.fr/decision/2011/2011164QPC.htm

### CGU : contenu minimal

- critères d'acceptation et de rejet, numérotés ;
- description de la modération algorithmique : d3-mini, second avis Mistral en zone grise, signalements traités par IA, aucune relecture humaine systématique ;
- mesures possibles (rejet, masquage, mise à jour), durées (purge à 30 jours) et voies de recours (réexamen humain via le lien de suivi, contact, juge) ;
- principaux paramètres du score de classement ;
- clause de licence (section 1) ;
- interdits : nommer ou rendre identifiable une personne physique, injurier ou diffamer, promouvoir ou dénigrer un candidat ou un parti ;
- rédaction en langage clair (art. 14(1) du DSA).

### Politique de confidentialité : plan

1. **Responsable du traitement** : identité et contact (confidentialite@iapolitique.fr). Pas de DPO, qui n'est pas obligatoire.
2. **Principes** : pas de compte, pas d'e-mail, pas de cookie, pas de publicité, pas de mesure d'audience tierce. Serveurs en France.
3. **Tableau des traitements** (finalité, base légale, durée) :
   - **Textes soumis** : finalité publication et analyse ; base intérêt légitime (art. 6(1)(f)). Pour d'éventuelles opinions politiques, art. 9(2)(e) : données rendues publiques par un choix éclairé de l'auteur (CJUE C-252/21 et C-446/21), d'où une case « je comprends que mon texte sera publié ». Durées : publiés, durée de vie du site ; rejetés, 30 jours ; masqués, [à fixer].
   - **Jeton de suivi** : seule son empreinte est conservée, supprimée avec la contribution.
   - **Anti-abus** : ALTCHA sans cookie et HMAC de l'IP avec un sel quotidien gardé en mémoire puis détruit (24 h maximum). À présenter comme une « pseudonymisation éphémère », et non comme « aucune donnée personnelle ».
   - **Journaux techniques** : contenu et durée selon l'arbitrage de la section 4.
   - **Demandes d'exercice des droits** : 1 an.
4. **Consigne aux auteurs** : ne pas signer, ne nommer personne. Le texte affiché est la version masquée.
5. **IA** : modération locale. Mistral AI (France) est sous-traitant au sens de l'art. 28 RGPD : DPA, refus de l'entraînement activé, inférence dans l'UE sauf la recherche web (sous-traitants listés sur trust.mistral.ai). Étiquetage selon l'art. 50 de l'AI Act. Un rejet n'a aucun effet juridique, et un réexamen humain est possible.
6. **Destinataires** : l'éditeur, Mistral AI, l'autorité judiciaire sur réquisition.
7. **Open data** : contenus publiés et masqués uniquement, sans identifiant technique.
8. **Stockage local** : à décrire s'il existe (par exemple les liens de suivi en localStorage).
9. **Sécurité et sauvegardes** : [30] jours.
10. **Vos droits** : la preuve d'auteur est le jeton (art. 11(2)). Un tiers nommé peut demander le retrait sans jeton (art. 14(5)(b) pour l'information des tiers). Réponse sous 1 mois. Réclamation possible auprès de la CNIL.
11. **Date de mise à jour.**

### Transparence IA (AI Act)

- **Dates** : l'art. 50 s'applique depuis le 02/08/2026. Le règlement (UE) 2026/1744 (« Omnibus IA », en vigueur depuis le 27/07/2026, https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=OJ:L_202601744) ne modifie que l'art. 50(7). Son délai pour l'art. 50(2), fixé au 02/12/2026, vaut seulement pour les systèmes mis sur le marché avant le 02/08/2026. **Il ne vous concerne pas.**
- **Exception éditoriale** : elle exige une relecture humaine réelle ET une personne qui assume la responsabilité éditoriale. Elle ne s'applique donc pas ici.
- **Exclusion « activité personnelle »** : la FAQ de la Commission (https://digital-strategy.ec.europa.eu/en/faqs/transparency-obligations-under-article-50-ai-act) la rend défendable pour un particulier sans revenus, mais seulement pour les obligations du déployeur. Elle ne couvre pas celles d'un fournisseur.
- **Référence pour le format** : le Code de pratique sur la transparence des contenus générés par IA, jugé adéquat (https://digital-strategy.ec.europa.eu/en/policies/code-practice-ai-generated-content).
- **Libellé à placer en tête de chaque bloc généré**, visible et accessible, avant le contenu :

> **Analyse générée automatiquement par intelligence artificielle (Mistral AI), sans relecture humaine.** Elle peut contenir des erreurs ; vérifiez les sources citées. Ce n'est pas une position officielle.

- **Texte de l'internaute** : il n'est pas visé par l'art. 50, mais doit être séparé visuellement sous le titre « Contribution d'un internaute (texte original, données personnelles masquées) ».

### Période électorale

- Fenêtre du référé « fausses informations » (art. L.163-2 du Code électoral, https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000039278631) : **du 1er janvier 2027 jusqu'au second tour** de la présidentielle, annoncée pour les 18 avril et 2 mai 2027 (décret de convocation non trouvé).
- Ne jamais présenter le score comme un sondage.
- Rejeter les contenus qui promeuvent ou dénigrent un candidat ou un parti.

---

## 4. Impacts produit (à reporter dans AGENTS.md)

### Pages et textes

- Créer les pages `/mentions-legales`, `/confidentialite`, `/regles` (CGU, modération et score) et `/licence` (réutilisation des données), en HTML, liées dans le pied de page de toutes les pages. Lier `/confidentialite` et `/regles` sous chaque formulaire.
- Ajouter un bandeau ou un pied de page persistant : « Site citoyen indépendant et non officiel, sans lien avec l'État ni aucune administration ».
- Interdire Marianne, bloc-marque, DSFR, couleurs ou typographies imitant les sites .gouv.fr, et tout vocabulaire administratif trompeur (« dossier », « validé par »).
- Sur chaque formulaire de contribution publiable (idée, amélioration, objection) :
  - case non pré-cochée renvoyant à la clause de licence ;
  - case « je comprends que mon texte sera publié s'il est accepté » ;
  - avertissement : « Ne signez pas, ne nommez aucun agent ni particulier » ;
  - mention : « Votre proposition sera évaluée automatiquement par une IA (modèle local, puis Mistral AI) ; si elle est acceptée, elle sera reformulée et analysée par IA » ;
  - envoi bloqué sans acceptation.

### Base de données

- Stocker `cgu_version` et `cgu_accepted_at` avec chaque contribution, sans IP ni HMAC.
- Ne pas stocker le HMAC dans les tables de contributions, ou le purger sous 48 h au maximum.

### Étiquetage IA

- Étiquette visible et accessible, avec `aria-label`, avant chaque bloc généré.
- Marquage lisible par machine : attribut `data-ai-generated="true"` et JSON-LD dans le HTML.
- À livrer dès la mise en ligne.

### Exports

- Fournir un fichier `LICENSE` (texte LO 2.0) et un fichier README ou de métadonnées contenant : `license=etalab-2.0`, producteur, `generated_at`, version du schéma, avertissement IA, exclusion des contenus de tiers, mention de données personnelles masquées, avertissement d'irrévocabilité et invitation à se resynchroniser.
- Schéma :
  - texte original : `source_type=user`, `attribution="Contribution anonyme – iapolitique.fr"` ;
  - champs générés : `ai_generated=true`, `ai_model`, `ai_provider`, `generated_at`, `analysis_version`, `human_reviewed=false` ;
  - pour chaque idée : `date_derniere_mise_a_jour` et une URL canonique.
- Contenu : seulement les éléments publiés, visibles et masqués de leurs données personnelles. Exclure les rejets, les contenus masqués ou en attente, les jetons et leurs hash, les HMAC, les journaux de modération, les signalements bruts et les horodatages fins (date au jour près).
- Sources de tiers : titre, URL, éditeur et date seulement, sans extraits longs.
- Régénération nocturne, et après chaque masquage ou suppression. Fichiers datés et versionnés.
- Page de téléchargement : licence, compatibilité CC BY 4.0, texte d'attribution prêt à copier avec la date, avertissement IA.

### Pipeline IA

- Détecter et masquer les données personnelles **avant** l'appel à Mistral et avant la publication : noms de personnes physiques (sauf élus ou ministres agissant dans leur fonction), e-mails, téléphones, adresses, signatures. Le texte « original » affiché est la version masquée, avec la mention « données personnelles masquées ».
- Faire rejeter par d3-mini les idées qui imputent des faits à une personne identifiable, ou qui promeuvent ou dénigrent un candidat ou un parti.
- Prompt Mistral : ne jamais attribuer de faits à une personne nommée, toujours sourcer les chiffres, paraphraser sans reproduire de longs passages, présenter le chiffrage comme une estimation.

### Mistral

- Compte payant, avec le réglage « Anonymous improvement data » désactivé (Admin > Privacy). Conserver une capture d'écran. Sans ce refus, Mistral devient responsable de traitement distinct (DPA art. 2.3).
- Ne pas utiliser de modèles « Labs » ou « Preview » (CGV art. 4.3).
- Utiliser `api.eu.mistral.ai` pour les appels sans outil. La recherche web intégrée sort de la garantie régionale. Alternative : SearXNG auto-hébergé, branché via function calling.
- Demander la rétention zéro (ZDR) au support.
- Ne jamais transmettre IP, HMAC ou jeton.
- Archiver le DPA daté.

### Anti-abus et jetons

- Sel HMAC uniquement en mémoire : rotation toutes les 24 h puis destruction, jamais sur disque, dans un log ou dans une sauvegarde. Compteurs de limitation avec une durée de vie de 24 h au plus.
- **Docker Swarm** : publier les ports 80/443 de Traefik en `mode: host` (ou utiliser le PROXY protocol). Avec le routing mesh ingress, Traefik voit une IP interne et la limitation de débit par HMAC(IP) ne fonctionne pas (https://docs.docker.com/engine/swarm/ingress/).
- Jeton de suivi : au moins 128 bits aléatoires, seul son hash SHA-256 en base, affiché une seule fois, placé dans le **fragment `#`** de l'URL, avec l'en-tête `Referrer-Policy: no-referrer` sur la page de suivi.
- Page de suivi :
  - boutons « Supprimer ma contribution » (effacement immédiat et retrait des exports) et « Demander un réexamen humain » ;
  - exposé des motifs selon l'art. 17(3) du DSA : mesure prise et durée, article des règles en cause, « décision prise automatiquement par une IA », date de purge, voies de recours.

### Journaux et conservation (décision à prendre puis à documenter)

- **Option par défaut (recommandée)** :
  - l'application ne collecte jamais l'IP en clair ;
  - les journaux d'accès Traefik suppriment ou tronquent `ClientHost`/`ClientAddr` et retirent les en-têtes sensibles, ou sont conservés au plus 7 jours ;
  - les journaux d'administration sont conservés 6 à 12 mois (recommandation CNIL 2021-122) ;
  - le raisonnement est documenté dans le registre. D'une part, le site est probablement hors du champ du DSA et du statut d'hébergeur LCEN. D'autre part, le décret 2021-1362 art. 8 limite la conservation aux données « collectées » (CE, 30/06/2023, n° 459724, https://www.conseil-etat.fr/fr/arianeweb/CE/decision/2023-06-30/459724).
- **Option prudente** : un journal séparé et chiffré des seules requêtes POST de création, modification ou suppression (IP, port, horodatage, identifiant du contenu), conservé 1 an, accessible uniquement sur réquisition, avec purge automatique. La promesse « IP jamais stockée » doit alors être retirée et la politique de confidentialité adaptée (base légale : art. 6(1)(c) RGPD).
- **Purges automatiques** : rejets à 30 jours ; signalements traités à 6 mois ; contenus masqués après [durée à fixer] ; sauvegardes à 30 jours.

### Signalement et back-office

- Bouton « Signaler » sur chaque idée : motif, URL, nom et e-mail facultatifs, déclaration de bonne foi, rappel de l'art. 6 VII LCEN, accusé de réception, mention du traitement automatisé.
- File d'escalade humaine dans le back-office LAN pour les cas suivants : diffamation, menace pour la vie (DSA art. 18), terrorisme, pédocriminalité, haine. Transmission manuelle à PHAROS (internet-signalement.gouv.fr). Délai cible : 24 h.
- Fonction d'administration « publier une réponse » attachée à une idée (droit de réponse sous 3 jours).
- Adresse contact@iapolitique.fr, lue par un humain, avec alertes (retrait administratif sous 24 h ; règlement TCO sous 1 h si vous êtes hébergeur). Adresse confidentialite@iapolitique.fr (ou un alias) pour les demandes RGPD, avec réponse sous 1 mois.

### Frontend et divers

- Aucune ressource tierce : polices, CDN et statistiques auto-hébergés. ALTCHA en vérification serveur auto-hébergée, sans cookie et sans offre SaaS (Sentinel ou filtre anti-spam hébergé).
- Documenter tout usage de localStorage.
- Afficher le score comme « score calculé par algorithme », avec un lien vers son explication.
- Vérifier que le domaine est enregistré au nom d'une personne physique, avec des coordonnées exactes, auprès du bureau d'enregistrement.
- Documentation interne : registre des traitements (environ 8 fiches), test de mise en balance de l'intérêt légitime, AIPD courte, registre des violations.

---

## 5. Points à faire valider par un juriste

1. **Champ du DSA et statut d'hébergeur LCEN** pour un particulier sans aucune rémunération. Cela détermine l'obligation de conserver l'IP 1 an (décret 2021-1362 art. 5, lié à l'art. L34-1 II bis 3° CPCE, sans injonction) et la portée de son art. 8 (« dans la mesure où elles sont collectées »). Une IP traitée en mémoire pour calculer le HMAC, ou présente dans des journaux Traefik de 7 jours, est-elle « collectée » ?
2. **Anonymat de l'éditeur et RGPD.** Avec l'option B, l'art. 13(1)(a) RGPD (« identité » du responsable de traitement) permet-il un responsable non nommé, ou un pseudonyme ? Si ce n'est pas le cas, l'option B perd l'essentiel de son intérêt, et l'option D devient la seule voie vers la discrétion.
3. **Portée de l'arrêt CJUE du 16/06/2026 (C-188/24 et C-190/24)** pour une pré-modération entièrement automatisée et un score de classement, au regard de l'art. 6 du DSA, de son art. 7 et de l'art. 93-3 al. 5 de la loi de 1982. Le texte intégral de l'arrêt n'a pas été lu.
4. **Qualification AI Act.** Le propriétaire qui assemble d3-mini et Mistral par API sous son nom est-il « fournisseur » (art. 50(2) applicable) ou seulement « déployeur » (exclusion de l'art. 2(10) possible pour l'art. 50(4)) ? Aucune source officielle ne le tranche. Le statut définitif des lignes directrices de la Commission sur l'art. 50 (20/07/2026) n'est pas confirmé.
5. **Accès à l'API Mistral pour un particulier.** Les CGV commerciales visent une « organization, company, or other entity ». Une restriction aux clients professionnels depuis août 2026 n'est rapportée que par un traqueur tiers, non vérifié. Si elle est confirmée, il faudra créer une association, ce qui change l'éditeur, le Concédant et le responsable de traitement.
6. **Validité de la clause de licence.** Suffit-elle au regard de l'art. L131-3 CPI ? Le consentement au masquage et à la reformulation par IA est-il suffisant au regard du droit moral (intégrité) ? Ce point est particulièrement sensible pour des textes très courts dont l'originalité est incertaine.
7. **Art. 9(2)(e) RGPD.** Une case « mon texte sera publié » suffit-elle à fonder la publication d'éventuelles opinions politiques révélées indirectement (CJUE OT, C-184/20) ?

Non vérifiés à ce jour, mais sans incidence attendue sur le projet : le contenu exact des modifications de l'art. 6 LCEN par la loi n° 2026-813, et le décret de convocation de la présidentielle de 2027.