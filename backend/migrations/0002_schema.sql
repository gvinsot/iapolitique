-- Complete schema of iapolitique (AGENTS.md §5 to §8).
--
-- Conventions:
-- * enumerations are `text` columns with a CHECK constraint; the allowed values mirror the
--   Rust enums of `backend/src/domain` (a test compares them);
-- * amounts of ideas are whole euros in `bigint`, AI costs are micro-euros in `bigint`;
-- * identifiers exposed indirectly are UUIDv7 (time ordered), internal logs use identities;
-- * no personal data: no IP, no user agent, no e-mail. Tracking tokens are stored hashed.
-- * migrations are additive only: never edit this file once deployed, add a new one.

CREATE FUNCTION set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

-- ---------------------------------------------------------------------------------------------
-- Ideas
-- ---------------------------------------------------------------------------------------------

CREATE TABLE ideas (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    -- State machine of AGENTS.md §5.2 (domain::status::IdeaStatus).
    status text NOT NULL DEFAULT 'submitted' CHECK (status IN (
        'submitted', 'screening', 'second_opinion', 'rejected', 'purged', 'accepted', 'dedup',
        'merged', 'queued_analysis', 'analyzing', 'analysis_failed', 'published', 'hidden'
    )),
    status_changed_at timestamptz NOT NULL DEFAULT now(),

    -- Submission (§5.1). The original text stays readable once published; it is purged 30 days
    -- after a rejection (§7.3).
    original_text text CHECK (char_length(original_text) BETWEEN 1 AND 4000),
    submitted_admin_level text CHECK (submitted_admin_level IN (
        'etat', 'securite_sociale', 'region', 'departement', 'intercommunalite', 'commune', 'multi'
    )),
    submitted_territory text CHECK (char_length(submitted_territory) BETWEEN 1 AND 200),
    rejection_reason_code text,
    rejected_at timestamptz,
    original_text_purged_at timestamptz,

    -- Duplicates (§5.4): a merged idea points to the published idea it was merged into.
    parent_idea_id uuid REFERENCES ideas (id),
    proposal_count integer NOT NULL DEFAULT 1 CHECK (proposal_count >= 1),
    contribution_count integer NOT NULL DEFAULT 0 CHECK (contribution_count >= 0),

    -- Publication: copied from the current analysis for listing, filtering and sorting.
    slug text UNIQUE CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$' AND char_length(slug) <= 80),
    current_analysis_id uuid,
    title text,
    summary text,
    admin_level text CHECK (admin_level IN (
        'etat', 'securite_sociale', 'region', 'departement', 'intercommunalite', 'commune', 'multi'
    )),
    territory_name text,
    territory_code text CHECK (territory_code ~ '^[0-9A-Z]{1,12}$'),
    category text,
    kind text CHECK (kind IN ('savings', 'revenue', 'mixed')),
    annual_gain_median_eur bigint CHECK (annual_gain_median_eur >= 0),
    implementation_cost_median_eur bigint CHECK (implementation_cost_median_eur >= 0),
    time_to_effect_months integer CHECK (time_to_effect_months BETWEEN 0 AND 600),
    feasibility smallint CHECK (feasibility BETWEEN 1 AND 5),
    confidence text CHECK (confidence IN ('low', 'medium', 'high')),
    -- Composite score and its components (§6.3, domain::scoring).
    score_eur bigint,
    net_annual_eur bigint,
    feasibility_factor double precision,
    confidence_factor double precision,
    delay_factor double precision,
    scoring_version text,
    first_published_at timestamptz,
    published_at timestamptz,
    hidden_at timestamptz,
    hidden_reason text,
    last_analyzed_at timestamptz,
    last_report_review_at timestamptz,

    -- Reserved, out of MVP scope (§11): real-world outcome of the idea.
    external_status text,
    external_status_note text,
    external_status_updated_at timestamptz,

    -- Full text search on the neutral title and summary (`q` filter).
    search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('french_unaccent'::regconfig, coalesce(title, '')), 'A')
        || setweight(to_tsvector('french_unaccent'::regconfig, coalesce(summary, '')), 'B')
    ) STORED,

    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT ideas_text_until_purge CHECK (status = 'purged' OR original_text IS NOT NULL),
    CONSTRAINT ideas_purged_without_text CHECK (status <> 'purged' OR original_text IS NULL),
    CONSTRAINT ideas_merged_has_parent CHECK ((status = 'merged') = (parent_idea_id IS NOT NULL)),
    CONSTRAINT ideas_parent_not_self CHECK (parent_idea_id <> id),
    CONSTRAINT ideas_publication_complete CHECK (
        status NOT IN ('published', 'hidden')
        OR (slug IS NOT NULL AND title IS NOT NULL AND summary IS NOT NULL
            AND current_analysis_id IS NOT NULL AND score_eur IS NOT NULL
            AND published_at IS NOT NULL)
    )
);

CREATE TRIGGER ideas_set_updated_at BEFORE UPDATE ON ideas
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Public listing (§13.2): one partial index per sort order, only published ideas.
CREATE INDEX ideas_published_score_idx ON ideas (score_eur DESC, id) WHERE status = 'published';
CREATE INDEX ideas_published_gain_idx ON ideas (annual_gain_median_eur DESC, id)
    WHERE status = 'published';
CREATE INDEX ideas_published_feasibility_idx ON ideas (feasibility DESC, score_eur DESC, id)
    WHERE status = 'published';
CREATE INDEX ideas_published_delay_idx ON ideas (time_to_effect_months, score_eur DESC, id)
    WHERE status = 'published';
CREATE INDEX ideas_published_recent_idx ON ideas (published_at DESC, id)
    WHERE status = 'published';
CREATE INDEX ideas_published_filters_idx ON ideas (admin_level, category, kind)
    WHERE status = 'published';
CREATE INDEX ideas_search_idx ON ideas USING gin (search_vector);
CREATE INDEX ideas_territory_trgm_idx ON ideas USING gin (territory_name gin_trgm_ops)
    WHERE territory_name IS NOT NULL;
-- Maintenance and pipeline.
CREATE INDEX ideas_status_idx ON ideas (status, status_changed_at);
CREATE INDEX ideas_rejected_idx ON ideas (rejected_at) WHERE status = 'rejected';
CREATE INDEX ideas_parent_idx ON ideas (parent_idea_id) WHERE parent_idea_id IS NOT NULL;

-- Timestamped history of every status change (§5.2).
CREATE TABLE idea_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    idea_id uuid NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    -- NULL for the creation event.
    from_status text,
    to_status text NOT NULL,
    actor text NOT NULL CHECK (actor IN ('author', 'api', 'worker', 'admin')),
    reason_code text,
    -- Internal identifiers only, never user text.
    details jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(details) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idea_events_idea_idx ON idea_events (idea_id, created_at, id);

-- ---------------------------------------------------------------------------------------------
-- Analyses (§6), versioned per idea
-- ---------------------------------------------------------------------------------------------

CREATE TABLE analyses (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    idea_id uuid NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    version integer NOT NULL CHECK (version >= 1),
    trigger text NOT NULL CHECK (trigger IN ('initial', 'contribution', 'merge', 'report', 'admin')),
    -- Full output validated against prompts/analysis.schema.json.
    content jsonb NOT NULL CHECK (jsonb_typeof(content) = 'object'),
    -- Extracted columns (§6.2).
    title text NOT NULL,
    summary text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('savings', 'revenue', 'mixed')),
    category text NOT NULL,
    admin_level text NOT NULL CHECK (admin_level IN (
        'etat', 'securite_sociale', 'region', 'departement', 'intercommunalite', 'commune', 'multi'
    )),
    territory_name text,
    territory_code text CHECK (territory_code ~ '^[0-9A-Z]{1,12}$'),
    legal_vehicle text NOT NULL CHECK (legal_vehicle IN (
        'loi', 'loi_de_finances', 'decret', 'arrete', 'deliberation', 'circulaire',
        'organisation_interne', 'autre'
    )),
    annual_gain_low_eur bigint NOT NULL,
    annual_gain_median_eur bigint NOT NULL,
    annual_gain_high_eur bigint NOT NULL,
    implementation_cost_low_eur bigint NOT NULL,
    implementation_cost_median_eur bigint NOT NULL,
    implementation_cost_high_eur bigint NOT NULL,
    time_to_effect_months integer NOT NULL CHECK (time_to_effect_months BETWEEN 0 AND 600),
    feasibility smallint NOT NULL CHECK (feasibility BETWEEN 1 AND 5),
    confidence text NOT NULL CHECK (confidence IN ('low', 'medium', 'high')),
    sources jsonb NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(sources) = 'array'),
    -- Score (§6.3).
    score_eur bigint NOT NULL,
    net_annual_eur bigint NOT NULL,
    feasibility_factor double precision NOT NULL,
    confidence_factor double precision NOT NULL,
    delay_factor double precision NOT NULL,
    -- Versions (§6.5).
    model text NOT NULL,
    prompt_version text NOT NULL,
    schema_version text NOT NULL,
    scoring_version text NOT NULL,
    web_search_count integer NOT NULL DEFAULT 0 CHECK (web_search_count >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT analyses_idea_version_key UNIQUE (idea_id, version),
    CONSTRAINT analyses_gain_range CHECK (
        annual_gain_low_eur >= 0
        AND annual_gain_low_eur <= annual_gain_median_eur
        AND annual_gain_median_eur <= annual_gain_high_eur
    ),
    CONSTRAINT analyses_cost_range CHECK (
        implementation_cost_low_eur >= 0
        AND implementation_cost_low_eur <= implementation_cost_median_eur
        AND implementation_cost_median_eur <= implementation_cost_high_eur
    )
);

ALTER TABLE ideas ADD CONSTRAINT ideas_current_analysis_fk
    FOREIGN KEY (current_analysis_id) REFERENCES analyses (id);

-- Embeddings for duplicate detection (§5.4): mistral-embed, 1024 dimensions, cosine distance.
CREATE TABLE idea_embeddings (
    idea_id uuid PRIMARY KEY REFERENCES ideas (id) ON DELETE CASCADE,
    model text NOT NULL,
    embedding vector(1024) NOT NULL,
    -- SHA-256 of the embedded text: avoids recomputing an unchanged embedding.
    content_sha256 bytea NOT NULL CHECK (octet_length(content_sha256) = 32),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idea_embeddings_hnsw_idx ON idea_embeddings
    USING hnsw (embedding vector_cosine_ops);

-- ---------------------------------------------------------------------------------------------
-- Contributions (§5.5) and reports (§5.6)
-- ---------------------------------------------------------------------------------------------

CREATE TABLE contributions (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    idea_id uuid NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('amelioration', 'objection', 'doublon')),
    status text NOT NULL DEFAULT 'submitted' CHECK (status IN (
        'submitted', 'screening', 'second_opinion', 'accepted', 'rejected', 'integrated', 'purged'
    )),
    status_changed_at timestamptz NOT NULL DEFAULT now(),
    -- Raw text, never published. NULL for merged duplicates (the text stays on the source idea).
    text text CHECK (char_length(text) BETWEEN 1 AND 4000),
    -- For `doublon`: the new idea merged into `idea_id`.
    source_idea_id uuid UNIQUE REFERENCES ideas (id),
    rejection_reason_code text,
    rejected_at timestamptz,
    text_purged_at timestamptz,
    -- Neutral summary written by the analysis that integrated it (shown on the portal).
    neutral_summary text,
    integrated_in_analysis_id uuid REFERENCES analyses (id),
    integrated_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT contributions_duplicate_source CHECK ((kind = 'doublon') = (source_idea_id IS NOT NULL)),
    CONSTRAINT contributions_text_until_purge CHECK (
        status = 'purged' OR kind = 'doublon' OR text IS NOT NULL
    ),
    CONSTRAINT contributions_purged_without_text CHECK (status <> 'purged' OR text IS NULL),
    CONSTRAINT contributions_integrated_analysis CHECK (
        status <> 'integrated' OR integrated_in_analysis_id IS NOT NULL
    )
);

CREATE TRIGGER contributions_set_updated_at BEFORE UPDATE ON contributions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE INDEX contributions_idea_idx ON contributions (idea_id, status, created_at);
CREATE INDEX contributions_rejected_idx ON contributions (rejected_at) WHERE status = 'rejected';

-- One AI review covers all the pending reports of an idea (at most one per idea per 24 h).
CREATE TABLE report_reviews (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    idea_id uuid NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    -- Analysis that was reviewed.
    analysis_id uuid REFERENCES analyses (id),
    outcome text NOT NULL CHECK (outcome IN ('maintain', 'update', 'hide')),
    explanation text NOT NULL,
    model text NOT NULL,
    prompt_version text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX report_reviews_idea_idx ON report_reviews (idea_id, created_at DESC);

CREATE TABLE reports (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    idea_id uuid NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    reason text NOT NULL CHECK (reason IN (
        'erreur_chiffrage', 'source_erronee', 'contenu_problematique', 'doublon', 'autre'
    )),
    -- Optional details typed by the visitor, never published.
    details text CHECK (char_length(details) BETWEEN 1 AND 2000),
    status text NOT NULL DEFAULT 'submitted' CHECK (status IN (
        'submitted', 'screening', 'second_opinion', 'accepted', 'rejected', 'reviewed', 'purged'
    )),
    status_changed_at timestamptz NOT NULL DEFAULT now(),
    rejection_reason_code text,
    rejected_at timestamptz,
    details_purged_at timestamptz,
    review_id uuid REFERENCES report_reviews (id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT reports_purged_without_details CHECK (status <> 'purged' OR details IS NULL),
    CONSTRAINT reports_reviewed_has_review CHECK (status <> 'reviewed' OR review_id IS NOT NULL)
);

CREATE TRIGGER reports_set_updated_at BEFORE UPDATE ON reports
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE INDEX reports_idea_idx ON reports (idea_id, status, created_at);
CREATE INDEX reports_rejected_idx ON reports (rejected_at) WHERE status = 'rejected';

-- ---------------------------------------------------------------------------------------------
-- Anonymous tracking links (§7.1): only the SHA-256 of the 256-bit token is stored.
-- ---------------------------------------------------------------------------------------------

CREATE TABLE tracking_tokens (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    target_kind text NOT NULL CHECK (target_kind IN ('idea', 'contribution', 'report')),
    target_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz
);

CREATE INDEX tracking_tokens_target_idx ON tracking_tokens (target_kind, target_id);
CREATE INDEX tracking_tokens_expiry_idx ON tracking_tokens (expires_at) WHERE expires_at IS NOT NULL;

-- ---------------------------------------------------------------------------------------------
-- Screenings by the decision model (§5.3), kept without text for threshold calibration.
-- ---------------------------------------------------------------------------------------------

CREATE TABLE screenings (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    target_kind text NOT NULL CHECK (target_kind IN ('idea', 'contribution', 'report')),
    target_id uuid NOT NULL,
    -- Version of prompts/screening.yaml and thresholds actually applied.
    prompt_version text NOT NULL,
    thresholds jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(thresholds) = 'object'),
    -- Raw `answers` of the decision service; NULL when the call failed.
    raw_answers jsonb,
    -- { criterion_key: probability of passing the criterion }.
    pass_probabilities jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(pass_probabilities) = 'object'),
    -- { criterion_key: "pass" | "fail" | "gray" }.
    criterion_verdicts jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(criterion_verdicts) = 'object'),
    -- NULL when the decision service failed (see error_code).
    outcome text CHECK (outcome IN ('accepted', 'rejected', 'gray_zone')),
    -- First failed criterion, in the order of screening.yaml.
    reason_code text,
    gray_zone boolean NOT NULL DEFAULT false,
    latency_ms integer CHECK (latency_ms >= 0),
    error_code text,
    -- Mistral second opinion for the gray zone.
    second_opinion_decision text CHECK (second_opinion_decision IN ('accept', 'reject')),
    second_opinion_reason_code text,
    second_opinion_explanation text,
    second_opinion_model text,
    second_opinion_prompt_version text,
    second_opinion_at timestamptz,
    -- Final decision once known (decision model or second opinion).
    final_decision text CHECK (final_decision IN ('accept', 'reject')),
    decided_at timestamptz,
    -- Admin label for calibration (§8): what the decision should have been.
    human_label text CHECK (human_label IN ('accept', 'reject')),
    human_label_note text,
    labeled_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT screenings_gray_zone_flag CHECK (gray_zone = (outcome IS NOT DISTINCT FROM 'gray_zone')),
    CONSTRAINT screenings_outcome_or_error CHECK (outcome IS NOT NULL OR error_code IS NOT NULL)
);

CREATE INDEX screenings_target_idx ON screenings (target_kind, target_id, created_at DESC);
CREATE INDEX screenings_created_idx ON screenings (created_at DESC);
CREATE INDEX screenings_unlabeled_idx ON screenings (created_at DESC)
    WHERE human_label IS NULL AND outcome IS NOT NULL;

-- ---------------------------------------------------------------------------------------------
-- Job queue (§3.1): consumed with SELECT … FOR UPDATE SKIP LOCKED.
-- ---------------------------------------------------------------------------------------------

CREATE TABLE jobs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind text NOT NULL CHECK (kind ~ '^[a-z][a-z0-9_]*$'),
    -- Internal identifiers only, never user text.
    payload jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(payload) = 'object'),
    -- Higher runs first (§6.4: second opinion > report > new idea > contribution > admin).
    priority smallint NOT NULL DEFAULT 0,
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'succeeded', 'dead')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts integer NOT NULL DEFAULT 5 CHECK (max_attempts >= 1),
    run_after timestamptz NOT NULL DEFAULT now(),
    locked_at timestamptz,
    locked_by text,
    last_error text,
    -- At most one queued or running job per key.
    dedupe_key text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,

    CONSTRAINT jobs_lock_iff_running CHECK (
        (status = 'running') = (locked_at IS NOT NULL AND locked_by IS NOT NULL)
    ),
    CONSTRAINT jobs_finished_iff_done CHECK (
        (status IN ('succeeded', 'dead')) = (finished_at IS NOT NULL)
    )
);

CREATE TRIGGER jobs_set_updated_at BEFORE UPDATE ON jobs
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE INDEX jobs_claim_idx ON jobs (priority DESC, run_after, id) WHERE status = 'queued';
CREATE UNIQUE INDEX jobs_dedupe_key_idx ON jobs (dedupe_key)
    WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running');
CREATE INDEX jobs_running_idx ON jobs (locked_at) WHERE status = 'running';
CREATE INDEX jobs_finished_idx ON jobs (finished_at) WHERE status IN ('succeeded', 'dead');
CREATE INDEX jobs_kind_status_idx ON jobs (kind, status);

-- Recurring jobs (daily purge, exports…): the worker that atomically moves `next_run_at`
-- forward enqueues the job, so each run happens once whatever the number of replicas.
CREATE TABLE job_schedules (
    name text PRIMARY KEY,
    next_run_at timestamptz NOT NULL,
    last_enqueued_at timestamptz,
    last_job_id bigint
);

-- ---------------------------------------------------------------------------------------------
-- AI calls and budget (§6.4)
-- ---------------------------------------------------------------------------------------------

CREATE TABLE ai_calls (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    provider text NOT NULL CHECK (provider IN ('decision_service', 'mistral')),
    purpose text NOT NULL CHECK (purpose IN (
        'screening', 'second_opinion', 'analysis_research', 'analysis_structuring', 'embedding',
        'report_review'
    )),
    model text NOT NULL,
    target_kind text CHECK (target_kind IN ('idea', 'contribution', 'report')),
    target_id uuid,
    idea_id uuid REFERENCES ideas (id) ON DELETE SET NULL,
    job_id bigint,
    prompt_version text,
    input_tokens integer NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    output_tokens integer NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    web_search_count integer NOT NULL DEFAULT 0 CHECK (web_search_count >= 0),
    cost_micro_eur bigint NOT NULL DEFAULT 0 CHECK (cost_micro_eur >= 0),
    latency_ms integer NOT NULL CHECK (latency_ms >= 0),
    status text NOT NULL CHECK (status IN ('success', 'error', 'timeout', 'invalid_output')),
    http_status smallint,
    error_code text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX ai_calls_created_idx ON ai_calls (created_at);
CREATE INDEX ai_calls_idea_idx ON ai_calls (idea_id, created_at) WHERE idea_id IS NOT NULL;

-- ---------------------------------------------------------------------------------------------
-- Rate limiting (§7.2): HMAC(daily salt, ip). The salt is random, renewed every day
-- (Europe/Paris) and destroyed the next day with its counters. The IP is never stored.
-- ---------------------------------------------------------------------------------------------

CREATE TABLE rate_limit_salts (
    day date PRIMARY KEY,
    salt bytea NOT NULL CHECK (octet_length(salt) = 32),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE rate_limit_counters (
    day date NOT NULL REFERENCES rate_limit_salts (day) ON DELETE CASCADE,
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    action text NOT NULL CHECK (action IN ('idea', 'contribution', 'report')),
    -- Start of the hour of the counted requests.
    window_start timestamptz NOT NULL,
    count integer NOT NULL DEFAULT 0 CHECK (count >= 0),
    PRIMARY KEY (day, fingerprint, action, window_start)
);

-- ---------------------------------------------------------------------------------------------
-- Administration (§8)
-- ---------------------------------------------------------------------------------------------

CREATE TABLE admin_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    action text NOT NULL CHECK (action ~ '^[a-z][a-z0-9_.]*$'),
    target_kind text,
    target_id text,
    details jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(details) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX admin_actions_created_idx ON admin_actions (created_at DESC);

-- Runtime settings adjustable from the admin (thresholds…), overriding the defaults of
-- prompts/ and of the environment.
CREATE TABLE settings (
    key text PRIMARY KEY CHECK (key ~ '^[a-z][a-z0-9_.]*$'),
    value jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER settings_set_updated_at BEFORE UPDATE ON settings
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------------------------
-- Open data exports (§7.4), regenerated daily. Stored in the database because replicas share
-- no file system.
-- ---------------------------------------------------------------------------------------------

CREATE TABLE exports (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    format text NOT NULL CHECK (format IN ('json', 'csv')),
    content bytea NOT NULL,
    content_sha256 bytea NOT NULL CHECK (octet_length(content_sha256) = 32),
    size_bytes bigint NOT NULL CHECK (size_bytes >= 0),
    idea_count integer NOT NULL CHECK (idea_count >= 0),
    scoring_version text NOT NULL,
    generated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX exports_latest_idx ON exports (format, generated_at DESC);
