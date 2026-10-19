-- Development seed: a few published ideas with complete analyses, to exercise the public pages
-- (ranking, idea page, statistics, exports) without a Mistral key.
--
-- NEVER run against a production or QA database. The data is fictitious: amounts, sources and
-- texts are invented for display tests and carry no meaning. See dev/README.md.
--
--   psql "$DATABASE_CONNECTION_STRING" -v ON_ERROR_STOP=1 -f dev/seed.sql
--
-- Re-runnable: the ideas it created before (slugs below) are deleted first. The api (or
-- `iapolitique migrate`) must have applied the migrations.

\set ON_ERROR_STOP on

DO $guard$
BEGIN
    IF current_database() ~* '(prod|qa)' THEN
        RAISE EXCEPTION 'dev/seed.sql refuses to run on database %', current_database();
    END IF;
END
$guard$;

BEGIN;

-- Earlier runs of this script.
DELETE FROM ideas WHERE slug IN (
    'mutualiser-achats-fournitures-communes',
    'supprimer-avis-imposition-papier-restants',
    'controles-cibles-arrets-de-travail-courts',
    'contrats-performance-energetique-lycees',
    'collecte-taxe-sejour-plateformes-location',
    'covoiturage-domicile-travail-agents-departement',
    'idee-masquee-exemple'
);

-- One published (or hidden) idea and its analysis; the score follows AGENTS.md §6.3.
CREATE FUNCTION pg_temp.seed_idea(
    p_slug text,
    p_status text,
    p_original text,
    p_content jsonb,
    p_proposals integer,
    p_published_days_ago integer
) RETURNS uuid
LANGUAGE plpgsql AS $fn$
DECLARE
    v_idea uuid := uuidv7();
    v_analysis uuid := uuidv7();
    v_gain bigint := (p_content #>> '{annual_gain_eur,median}')::bigint;
    v_cost bigint := (p_content #>> '{implementation_cost_eur,median}')::bigint;
    v_months integer := (p_content ->> 'time_to_effect_months')::integer;
    v_feasibility smallint := (p_content #>> '{feasibility,score}')::smallint;
    v_confidence text := p_content ->> 'confidence';
    v_confidence_factor numeric := CASE v_confidence
        WHEN 'low' THEN 0.5 WHEN 'medium' THEN 0.75 ELSE 1 END;
    v_net numeric := (5 * v_gain - v_cost) / 5.0;
    v_feasibility_factor numeric := v_feasibility / 5.0;
    v_delay numeric := 24.0 / (24 + v_months);
    v_score bigint := round(v_net * v_feasibility_factor * v_confidence_factor * v_delay);
    v_published timestamptz := now() - make_interval(days => p_published_days_ago);
    v_territory text := nullif(p_content #>> '{territory,name}', '');
    v_code text := nullif(p_content #>> '{territory,insee_code}', '');
BEGIN
    INSERT INTO ideas (
        id, status, original_text, submitted_admin_level, proposal_count, slug, title,
        summary, admin_level, territory_name, territory_code, category, kind,
        annual_gain_median_eur, implementation_cost_median_eur, time_to_effect_months,
        feasibility, confidence, score_eur, net_annual_eur, feasibility_factor,
        confidence_factor, delay_factor, scoring_version, first_published_at, published_at,
        hidden_at, hidden_reason, last_analyzed_at, cgu_version, cgu_accepted_at, created_at
    ) VALUES (
        -- Published once its analysis exists (CHECK ideas_publication_complete).
        v_idea, 'analyzing', p_original, p_content ->> 'admin_level', p_proposals, p_slug,
        p_content ->> 'title', p_content ->> 'summary', p_content ->> 'admin_level', v_territory,
        v_code, p_content ->> 'category', p_content ->> 'kind', v_gain, v_cost, v_months,
        v_feasibility, v_confidence, v_score, round(v_net), v_feasibility_factor,
        v_confidence_factor, v_delay, '1', v_published, v_published,
        NULL, NULL,
        v_published, '1.0', (v_published - interval '2 days')::date,
        v_published - interval '2 days'
    );

    INSERT INTO analyses (
        id, idea_id, version, trigger, content, title, summary, kind, category, admin_level,
        territory_name, territory_code, legal_vehicle, annual_gain_low_eur,
        annual_gain_median_eur, annual_gain_high_eur, implementation_cost_low_eur,
        implementation_cost_median_eur, implementation_cost_high_eur, time_to_effect_months,
        feasibility, confidence, sources, score_eur, net_annual_eur, feasibility_factor,
        confidence_factor, delay_factor, model, prompt_version, research_prompt_version,
        schema_version, scoring_version, web_search_count, created_at
    ) VALUES (
        v_analysis, v_idea, 1, 'initial', p_content, p_content ->> 'title',
        p_content ->> 'summary', p_content ->> 'kind', p_content ->> 'category',
        p_content ->> 'admin_level', v_territory, v_code, p_content ->> 'legal_vehicle',
        (p_content #>> '{annual_gain_eur,low}')::bigint, v_gain,
        (p_content #>> '{annual_gain_eur,high}')::bigint,
        (p_content #>> '{implementation_cost_eur,low}')::bigint, v_cost,
        (p_content #>> '{implementation_cost_eur,high}')::bigint, v_months, v_feasibility,
        v_confidence, p_content -> 'sources', v_score, round(v_net), v_feasibility_factor,
        v_confidence_factor, v_delay, 'mistral-large-latest', '1.1.0', '1.1.0',
        'sha256:000000000000', '1', 3, v_published
    );

    UPDATE ideas
    SET current_analysis_id = v_analysis, status = p_status,
        hidden_at = CASE WHEN p_status = 'hidden' THEN now() END,
        hidden_reason = CASE WHEN p_status = 'hidden' THEN 'admin' END
    WHERE id = v_idea;
    INSERT INTO idea_events (idea_id, from_status, to_status, actor, created_at)
    VALUES (v_idea, NULL, 'submitted', 'api', v_published - interval '2 days'),
           (v_idea, 'analyzing', 'published', 'worker', v_published);
    RETURN v_idea;
END
$fn$;

-- Impacts for the nine publics of the taxonomy, in its order.
CREATE FUNCTION pg_temp.impacts(effects text[], explanations text[]) RETURNS jsonb
LANGUAGE sql AS $fn$
    SELECT jsonb_object_agg(public, jsonb_build_object('effect', effects[n], 'explanation', explanations[n]))
    FROM unnest(ARRAY[
        'menages_modestes', 'classes_moyennes', 'menages_aises', 'entreprises', 'agents_publics',
        'usagers_services_publics', 'territoires_ruraux', 'territoires_urbains', 'environnement'
    ]) WITH ORDINALITY AS t(public, n)
$fn$;

DO $seed$
DECLARE
    v_purchases uuid;
    v_sick_leave uuid;
    v_contribution_a uuid := uuidv7();
    v_contribution_b uuid := uuidv7();
BEGIN
    v_purchases := pg_temp.seed_idea(
        'mutualiser-achats-fournitures-communes', 'published',
        E'Les petites communes achètent chacune leurs fournitures (papier, produits d''entretien, petit matériel) à des prix élevés. Le département pourrait proposer une centrale d''achat commune pour obtenir de meilleurs tarifs et alléger les procédures.',
        jsonb_build_object(
            'title', 'Mutualiser les achats de fournitures des communes d’un même département',
            'summary', 'Regrouper les commandes de fournitures courantes des communes volontaires au sein d’une centrale d’achat départementale pour obtenir de meilleurs prix et réduire les coûts de procédure.',
            'improved_proposal', E'Créer une centrale d’achat départementale facultative, ouverte aux communes volontaires et à leurs établissements publics, avec un catalogue commun de fournitures courantes [S1].\nLes communes gardent la liberté de commander hors catalogue pour les achats de proximité.',
            'existing_situation', 'Des groupements de commandes existent déjà dans plusieurs départements, sans couvrir l’ensemble des communes [S2] ; le gain chiffré est supplémentaire.',
            'admin_level', 'commune',
            'territory', jsonb_build_object('name', 'Gironde', 'insee_code', '33'),
            'category', 'achats_publics',
            'kind', 'savings',
            'annual_gain_eur', jsonb_build_object('low', 2000000, 'median', 5000000, 'high', 9000000,
                'calculation', 'Dépenses de fournitures des communes concernées (environ 50 M€ par an) × 10 % d’économie moyenne sur les prix [S1], avec un taux d’adhésion de 60 % à 100 %.'),
            'implementation_cost_eur', jsonb_build_object('low', 500000, 'median', 1000000, 'high', 2000000,
                'calculation', 'Mise en place de la centrale (2 à 4 agents pendant un an) et du catalogue [S2].'),
            'time_to_effect_months', 12,
            'legal_vehicle', 'deliberation',
            'feasibility', jsonb_build_object('score', 4, 'obstacles', jsonb_build_array('Adhésion volontaire des communes', 'Préparation d’un premier marché')),
            'confidence', 'medium',
            'confidence_explanation', 'Les dépenses de fournitures des communes sont publiées de façon partielle [S1].',
            'assumptions', jsonb_build_array('Taux d’adhésion entre 60 % et 100 % des communes [S1]', 'Économie moyenne de 10 % sur les prix unitaires [S2]'),
            'pros', jsonb_build_array('Prix unitaires plus bas grâce aux volumes [S1]', 'Procédures allégées pour les petites communes'),
            'cons', jsonb_build_array('Moins de choix pour les achats auprès de commerces locaux [S2]', 'Délais de livraison parfois plus longs'),
            'risks', jsonb_build_array('Dépendance à un nombre réduit de fournisseurs'),
            'impacts', pg_temp.impacts(
                ARRAY['neutral', 'neutral', 'neutral', 'uncertain', 'positive', 'neutral', 'positive', 'neutral', 'positive'],
                ARRAY['Pas d’effet direct sur les ménages modestes.', 'Pas d’effet direct.', 'Pas d’effet direct.',
                      'Les grands fournisseurs gagnent des volumes, les petits commerces locaux peuvent en perdre [S2].',
                      'Moins de procédures à préparer pour les secrétaires de mairie.', 'Pas d’effet visible sur les services rendus.',
                      'Les petites communes rurales bénéficient le plus des volumes groupés [S1].', 'Effet faible, les villes achetant déjà en volume.',
                      'Livraisons regroupées, moins de transport.']),
            'sources', jsonb_build_array(
                jsonb_build_object('id', 'S1', 'title', 'Les finances publiques locales (rapport annuel)', 'publisher', 'Cour des comptes', 'url', 'https://www.ccomptes.fr/fr/publications', 'used_for', 'Ordre de grandeur des dépenses de fonctionnement des communes'),
                jsonb_build_object('id', 'S2', 'title', 'Les groupements de commandes', 'publisher', 'Direction des affaires juridiques', 'url', 'https://www.economie.gouv.fr/daj', 'used_for', 'Situation existante et cadre juridique')),
            'contributions_considered', jsonb_build_array(
                jsonb_build_object('contribution_id', v_contribution_a::text, 'summary', 'Ouvrir la centrale aux établissements publics locaux (CCAS, syndicats).', 'taken_into_account', true, 'response', 'Intégré dans la proposition améliorée.'),
                jsonb_build_object('contribution_id', v_contribution_b::text, 'summary', 'Risque pour les commerces de proximité.', 'taken_into_account', false, 'response', 'Les communes gardent la possibilité de commander hors catalogue.')),
            'changes_since_previous', ''
        ),
        3, 6);

    INSERT INTO contributions (id, idea_id, kind, status, text, neutral_summary,
                               integrated_in_analysis_id, integrated_at, cgu_version, cgu_accepted_at)
    SELECT c.id, v_purchases, c.kind, 'integrated', c.text, c.summary, i.current_analysis_id,
           now() - interval '6 days', '1.0', current_date - 7
    FROM ideas i, (VALUES
        (v_contribution_a, 'amelioration', 'Il faudrait aussi ouvrir la centrale aux CCAS et aux syndicats intercommunaux.', 'Ouvrir la centrale aux établissements publics locaux (CCAS, syndicats).'),
        (v_contribution_b, 'objection', 'Les petits commerces du village vont perdre les commandes de la mairie.', 'Risque pour les commerces de proximité.')
    ) AS c(id, kind, text, summary)
    WHERE i.id = v_purchases;
    UPDATE ideas SET contribution_count = 2 WHERE id = v_purchases;

    PERFORM pg_temp.seed_idea(
        'supprimer-avis-imposition-papier-restants', 'published',
        'Beaucoup de contribuables reçoivent encore leur avis d''impôt sur papier alors qu''ils déclarent en ligne. Envoyer l''avis uniquement dans l''espace en ligne, sauf demande contraire, économiserait l''impression et l''affranchissement.',
        jsonb_build_object(
            'title', 'Envoyer les avis d’imposition en ligne par défaut aux usagers qui déclarent en ligne',
            'summary', 'Ne plus imprimer ni affranchir les avis d’imposition des contribuables qui déclarent déjà en ligne, sauf demande expresse de leur part.',
            'improved_proposal', 'Rendre l’avis dématérialisé par défaut pour les déclarants en ligne, avec une case simple pour continuer à recevoir le papier, et maintenir l’envoi papier pour les usagers sans accès au numérique [S1].',
            'existing_situation', 'Les déclarants en ligne peuvent déjà renoncer au papier, mais l’option doit être choisie ; une part importante des avis reste imprimée [S1].',
            'admin_level', 'etat',
            'territory', jsonb_build_object('name', '', 'insee_code', ''),
            'category', 'finances_publiques',
            'kind', 'savings',
            'annual_gain_eur', jsonb_build_object('low', 15000000, 'median', 30000000, 'high', 45000000,
                'calculation', 'Nombre d’avis encore imprimés pour des déclarants en ligne × coût unitaire d’impression et d’affranchissement (environ 0,60 €) [S1] [S2].'),
            'implementation_cost_eur', jsonb_build_object('low', 5000000, 'median', 8000000, 'high', 12000000,
                'calculation', 'Adaptation des applications et campagne d’information [S2].'),
            'time_to_effect_months', 18,
            'legal_vehicle', 'arrete',
            'feasibility', jsonb_build_object('score', 4, 'obstacles', jsonb_build_array('Accompagnement des usagers éloignés du numérique')),
            'confidence', 'high',
            'confidence_explanation', 'Les volumes et coûts unitaires sont publiés dans les documents budgétaires [S2].',
            'assumptions', jsonb_build_array('50 % des avis papier concernent des déclarants en ligne [S1]', 'Coût unitaire de 0,60 € [S2]'),
            'pros', jsonb_build_array('Économie récurrente sur l’impression et l’affranchissement [S2]', 'Moins de papier'),
            'cons', jsonb_build_array('Risque de non-lecture de l’avis par certains usagers'),
            'risks', jsonb_build_array('Exclusion des usagers sans accès régulier au numérique si l’option papier est mal connue'),
            'impacts', pg_temp.impacts(
                ARRAY['uncertain', 'neutral', 'neutral', 'neutral', 'positive', 'neutral', 'uncertain', 'neutral', 'positive'],
                ARRAY['Effet incertain pour les ménages moins équipés en numérique.', 'Pas d’effet notable.', 'Pas d’effet notable.', 'Pas d’effet.',
                      'Moins de traitements d’envois et de retours de courrier.', 'Service inchangé, avis disponible en ligne.',
                      'Couverture numérique parfois plus faible.', 'Pas d’effet notable.', 'Moins de papier et de transport de courrier.']),
            'sources', jsonb_build_array(
                jsonb_build_object('id', 'S1', 'title', 'Rapport annuel de performance « Gestion des finances publiques »', 'publisher', 'Ministère chargé des comptes publics', 'url', 'https://www.budget.gouv.fr/documentation/documents-budgetaires', 'used_for', 'Volumes d’avis et part des déclarants en ligne'),
                jsonb_build_object('id', 'S2', 'title', 'Projet de loi de finances, annexes', 'publisher', 'Ministère chargé des comptes publics', 'url', 'https://www.budget.gouv.fr/documentation/documents-budgetaires', 'used_for', 'Coûts d’affranchissement')),
            'contributions_considered', jsonb_build_array(),
            'changes_since_previous', ''
        ),
        2, 12);

    v_sick_leave := pg_temp.seed_idea(
        'controles-cibles-arrets-de-travail-courts', 'published',
        'Les arrêts de travail courts et répétés coûtent cher à l''Assurance maladie. Des contrôles mieux ciblés sur les situations atypiques permettraient de réduire les abus sans pénaliser les malades.',
        jsonb_build_object(
            'title', 'Cibler les contrôles des arrêts de travail courts et répétés',
            'summary', 'Concentrer les contrôles médicaux de l’Assurance maladie sur les arrêts courts et répétés présentant des profils atypiques, à partir des données déjà disponibles.',
            'improved_proposal', 'Utiliser des critères objectifs (fréquence, durée, cumul) pour orienter les contrôles, avec un examen médical systématique avant toute suspension d’indemnités [S1].',
            'existing_situation', 'Des contrôles existent mais restent peu ciblés ; les indemnités journalières progressent plus vite que l’emploi [S1] [S2].',
            'admin_level', 'securite_sociale',
            'territory', jsonb_build_object('name', '', 'insee_code', ''),
            'category', 'lutte_fraude',
            'kind', 'savings',
            'annual_gain_eur', jsonb_build_object('low', 50000000, 'median', 120000000, 'high', 250000000,
                'calculation', 'Indemnités journalières des arrêts courts (environ 3 Md€) × 2 % à 8 % d’arrêts injustifiés évités [S2].'),
            'implementation_cost_eur', jsonb_build_object('low', 10000000, 'median', 20000000, 'high', 40000000,
                'calculation', 'Recrutement de médecins-conseils et outils de ciblage [S1].'),
            'time_to_effect_months', 24,
            'legal_vehicle', 'decret',
            'feasibility', jsonb_build_object('score', 3, 'obstacles', jsonb_build_array('Disponibilité des médecins-conseils', 'Protection des données de santé')),
            'confidence', 'medium',
            'confidence_explanation', 'La part d’arrêts injustifiés est estimée à partir de contrôles par échantillon [S2].',
            'assumptions', jsonb_build_array('Arrêts injustifiés évités : 2 % à 8 % [S2]'),
            'pros', jsonb_build_array('Réduction des dépenses d’indemnités journalières [S1]', 'Équité entre assurés'),
            'cons', jsonb_build_array('Charge supplémentaire pour les médecins-conseils', 'Risque de contrôles ressentis comme intrusifs'),
            'risks', jsonb_build_array('Effet dissuasif sur des arrêts justifiés si les critères sont mal calibrés'),
            'impacts', pg_temp.impacts(
                ARRAY['uncertain', 'uncertain', 'neutral', 'positive', 'neutral', 'neutral', 'neutral', 'neutral', 'neutral'],
                ARRAY['Risque de renoncement à des arrêts justifiés si les contrôles sont mal compris.', 'Effet incertain selon le ciblage.', 'Pas d’effet notable.',
                      'Moins d’absences répétées non justifiées [S2].', 'Pas d’effet direct.', 'Pas d’effet direct.', 'Pas d’effet notable.', 'Pas d’effet notable.', 'Pas d’effet.']),
            'sources', jsonb_build_array(
                jsonb_build_object('id', 'S1', 'title', 'Rapport sur l’application des lois de financement de la sécurité sociale', 'publisher', 'Cour des comptes', 'url', 'https://www.ccomptes.fr/fr/publications', 'used_for', 'Évolution des indemnités journalières'),
                jsonb_build_object('id', 'S2', 'title', 'Charges et produits de l’Assurance maladie', 'publisher', 'Caisse nationale de l’Assurance maladie', 'url', 'https://www.assurance-maladie.ameli.fr/etudes-et-donnees', 'used_for', 'Montant des indemnités des arrêts courts')),
            'contributions_considered', jsonb_build_array(),
            'changes_since_previous', ''
        ),
        1, 4);

    INSERT INTO rights_of_reply (idea_id, author_label, body, requested_on, published_at)
    VALUES (v_sick_leave, 'une association de patients',
            E'Nous rappelons que la très grande majorité des arrêts de travail sont justifiés.\nTout ciblage doit reposer sur un examen médical et non sur des statistiques seules.',
            current_date - 3, now() - interval '2 days');

    PERFORM pg_temp.seed_idea(
        'contrats-performance-energetique-lycees', 'published',
        'Les lycées consomment beaucoup de chauffage. La région pourrait signer des contrats de performance énergétique où l''entreprise est payée sur les économies réalisées.',
        jsonb_build_object(
            'title', 'Rénover les lycées par des contrats de performance énergétique',
            'summary', 'Confier la rénovation énergétique d’un ensemble de lycées à des contrats rémunérés sur les économies d’énergie réellement mesurées.',
            'improved_proposal', 'Lancer des contrats de performance énergétique par lots de lycées, avec garantie de résultat mesurée chaque année [S1].',
            'existing_situation', 'Quelques régions expérimentent ces contrats ; la plupart des rénovations restent financées en travaux classiques [S1].',
            'admin_level', 'region',
            'territory', jsonb_build_object('name', 'Occitanie', 'insee_code', '76'),
            'category', 'immobilier_public',
            'kind', 'savings',
            'annual_gain_eur', jsonb_build_object('low', 8000000, 'median', 15000000, 'high', 25000000,
                'calculation', 'Factures d’énergie des lycées (environ 60 M€ par an) × 15 % à 40 % d’économie garantie [S1].'),
            'implementation_cost_eur', jsonb_build_object('low', 60000000, 'median', 90000000, 'high', 120000000,
                'calculation', 'Travaux de rénovation sur le périmètre retenu [S1].'),
            'time_to_effect_months', 36,
            'legal_vehicle', 'deliberation',
            'feasibility', jsonb_build_object('score', 3, 'obstacles', jsonb_build_array('Montage contractuel complexe', 'Travaux en site occupé')),
            'confidence', 'medium',
            'confidence_explanation', 'Les retours d’expérience sont encore peu nombreux [S1].',
            'assumptions', jsonb_build_array('Économie d’énergie garantie de 15 % à 40 % [S1]'),
            'pros', jsonb_build_array('Économies garanties par contrat', 'Baisse des émissions'),
            'cons', jsonb_build_array('Investissement initial élevé', 'Engagement de long terme'),
            'risks', jsonb_build_array('Défaillance du titulaire'),
            'impacts', pg_temp.impacts(
                ARRAY['neutral', 'neutral', 'neutral', 'positive', 'neutral', 'positive', 'neutral', 'neutral', 'positive'],
                ARRAY['Pas d’effet direct.', 'Pas d’effet direct.', 'Pas d’effet direct.', 'Activité pour les entreprises du bâtiment.',
                      'Pas d’effet notable.', 'Meilleur confort thermique pour les élèves.', 'Pas d’effet spécifique.', 'Pas d’effet spécifique.',
                      'Baisse des consommations d’énergie [S1].']),
            'sources', jsonb_build_array(
                jsonb_build_object('id', 'S1', 'title', 'Guide des contrats de performance énergétique', 'publisher', 'Agence de la transition écologique', 'url', 'https://librairie.ademe.fr', 'used_for', 'Niveaux d’économie garantis')),
            'contributions_considered', jsonb_build_array(),
            'changes_since_previous', ''
        ),
        1, 20);

    PERFORM pg_temp.seed_idea(
        'collecte-taxe-sejour-plateformes-location', 'published',
        'La taxe de séjour n''est pas toujours collectée sur les locations entre particuliers. La commune pourrait passer une convention de collecte avec toutes les plateformes.',
        jsonb_build_object(
            'title', 'Généraliser la collecte de la taxe de séjour par les plateformes de location',
            'summary', 'Conventionner avec les plateformes de location de courte durée pour qu’elles collectent et reversent la taxe de séjour de chaque nuitée.',
            'improved_proposal', 'Délibérer sur un tarif unique et vérifier chaque trimestre les reversements des plateformes à partir des déclarations des loueurs [S1].',
            'existing_situation', 'Les grandes plateformes collectent déjà la taxe dans de nombreuses communes, mais pas toutes ni pour tous les hébergements [S1].',
            'admin_level', 'commune',
            'territory', jsonb_build_object('name', 'Annecy', 'insee_code', '74010'),
            'category', 'fiscalite_locale',
            'kind', 'revenue',
            'annual_gain_eur', jsonb_build_object('low', 300000, 'median', 800000, 'high', 1500000,
                'calculation', 'Nuitées en location de courte durée non déclarées × tarif moyen de la taxe [S1].'),
            'implementation_cost_eur', jsonb_build_object('low', 50000, 'median', 100000, 'high', 200000,
                'calculation', 'Temps d’agent pour le suivi des reversements.'),
            'time_to_effect_months', 6,
            'legal_vehicle', 'deliberation',
            'feasibility', jsonb_build_object('score', 5, 'obstacles', jsonb_build_array()),
            'confidence', 'high',
            'confidence_explanation', 'Les nuitées sont connues par les déclarations des loueurs [S1].',
            'assumptions', jsonb_build_array('Part des nuitées non taxées : 20 % [S1]'),
            'pros', jsonb_build_array('Recette supplémentaire sans hausse de tarif', 'Égalité avec l’hôtellerie'),
            'cons', jsonb_build_array('Suivi administratif des reversements'),
            'risks', jsonb_build_array(),
            'impacts', pg_temp.impacts(
                ARRAY['neutral', 'neutral', 'neutral', 'uncertain', 'neutral', 'neutral', 'neutral', 'positive', 'neutral'],
                ARRAY['Pas d’effet.', 'Pas d’effet.', 'Pas d’effet.', 'Effet incertain pour les loueurs professionnels.', 'Pas d’effet notable.',
                      'Pas d’effet.', 'Pas d’effet.', 'Recette pour les communes touristiques [S1].', 'Pas d’effet.']),
            'sources', jsonb_build_array(
                jsonb_build_object('id', 'S1', 'title', 'Fiscalité locale : la taxe de séjour', 'publisher', 'Direction générale des collectivités locales', 'url', 'https://www.collectivites-locales.gouv.fr', 'used_for', 'Règles de collecte par les plateformes')),
            'contributions_considered', jsonb_build_array(),
            'changes_since_previous', ''
        ),
        4, 2);

    PERFORM pg_temp.seed_idea(
        'covoiturage-domicile-travail-agents-departement', 'published',
        'Le département rembourse beaucoup de frais de déplacement. Encourager le covoiturage entre agents pour les trajets domicile-travail réduirait les dépenses de véhicules de service.',
        jsonb_build_object(
            'title', 'Développer le covoiturage domicile-travail des agents départementaux',
            'summary', 'Mettre en relation les agents d’un même secteur pour leurs trajets et réduire le parc de véhicules de service sous-utilisés.',
            'improved_proposal', 'Ouvrir une plateforme interne de covoiturage et réduire progressivement le parc de véhicules de service selon son taux d’utilisation [S1].',
            'existing_situation', 'Le forfait mobilités durables existe déjà pour les agents publics [S1].',
            'admin_level', 'departement',
            'territory', jsonb_build_object('name', 'Ille-et-Vilaine', 'insee_code', '35'),
            'category', 'organisation_effectifs',
            'kind', 'mixed',
            'annual_gain_eur', jsonb_build_object('low', 200000, 'median', 500000, 'high', 1000000,
                'calculation', 'Réduction de 5 % à 15 % du parc de véhicules de service [S1].'),
            'implementation_cost_eur', jsonb_build_object('low', 100000, 'median', 200000, 'high', 400000,
                'calculation', 'Plateforme et animation.'),
            'time_to_effect_months', 12,
            'legal_vehicle', 'organisation_interne',
            'feasibility', jsonb_build_object('score', 4, 'obstacles', jsonb_build_array('Horaires variables des agents')),
            'confidence', 'low',
            'confidence_explanation', 'Peu de données publiques sur l’utilisation des parcs départementaux.',
            'assumptions', jsonb_build_array('Réduction du parc de 5 % à 15 %'),
            'pros', jsonb_build_array('Moins de véhicules à entretenir', 'Moins d’émissions'),
            'cons', jsonb_build_array('Contraintes d’horaires'),
            'risks', jsonb_build_array('Faible adhésion des agents'),
            'impacts', pg_temp.impacts(
                ARRAY['neutral', 'neutral', 'neutral', 'neutral', 'positive', 'neutral', 'uncertain', 'neutral', 'positive'],
                ARRAY['Pas d’effet.', 'Pas d’effet.', 'Pas d’effet.', 'Pas d’effet.', 'Frais de trajet partagés.', 'Pas d’effet.',
                      'Moins de solutions de covoiturage dans les zones peu denses.', 'Pas d’effet.', 'Moins de trajets en voiture individuelle.']),
            'sources', jsonb_build_array(
                jsonb_build_object('id', 'S1', 'title', 'Le forfait mobilités durables dans la fonction publique', 'publisher', 'Direction générale de l’administration et de la fonction publique', 'url', 'https://www.fonction-publique.gouv.fr', 'used_for', 'Cadre existant')),
            'contributions_considered', jsonb_build_array(),
            'changes_since_previous', ''
        ),
        1, 30);

    -- Masked by the administrator: must answer 404 on the portal.
    PERFORM pg_temp.seed_idea(
        'idee-masquee-exemple', 'hidden',
        'Texte d''une idée masquée pour tester la page 404 et la file de modération.',
        jsonb_build_object(
            'title', 'Idée masquée (exemple)', 'summary', 'Idée masquée par l’administration pour les tests.',
            'improved_proposal', '', 'existing_situation', '', 'admin_level', 'etat',
            'territory', jsonb_build_object('name', '', 'insee_code', ''), 'category', 'finances_publiques',
            'kind', 'savings',
            'annual_gain_eur', jsonb_build_object('low', 1000, 'median', 2000, 'high', 3000, 'calculation', ''),
            'implementation_cost_eur', jsonb_build_object('low', 0, 'median', 0, 'high', 0, 'calculation', ''),
            'time_to_effect_months', 12, 'legal_vehicle', 'autre',
            'feasibility', jsonb_build_object('score', 3, 'obstacles', jsonb_build_array()),
            'confidence', 'low', 'confidence_explanation', '', 'assumptions', jsonb_build_array(),
            'pros', jsonb_build_array(), 'cons', jsonb_build_array(), 'risks', jsonb_build_array(),
            'impacts', pg_temp.impacts(ARRAY['neutral','neutral','neutral','neutral','neutral','neutral','neutral','neutral','neutral'],
                                       ARRAY['','','','','','','','','']),
            'sources', jsonb_build_array(), 'contributions_considered', jsonb_build_array(),
            'changes_since_previous', ''
        ),
        1, 40);
END
$seed$;

-- Open data files: generated by the worker (job generate_exports) from the ideas above.
INSERT INTO jobs (kind, payload, dedupe_key)
SELECT 'generate_exports', '{}', 'generate_exports'
WHERE NOT EXISTS (
    SELECT 1 FROM jobs WHERE dedupe_key = 'generate_exports' AND status IN ('queued', 'running')
);

COMMIT;
