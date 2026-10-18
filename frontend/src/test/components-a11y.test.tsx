import { screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { DashboardTiles } from "@/components/admin/dashboard-tiles";
import {
  EscalationsTable,
  HiddenIdeasTable,
  ReportsTable,
  ReviewRequestsTable,
} from "@/components/admin/moderation-tables";
import { RightOfReplyEditor } from "@/components/admin/right-of-reply-editor";
import { ScreeningDecisions } from "@/components/admin/screening-decisions";
import { ThresholdSettingsForm } from "@/components/admin/threshold-settings-form";
import { AiGenerated } from "@/components/ideas/ai-generated-notice";
import { AssumptionsList, ProsCons } from "@/components/ideas/analysis-lists";
import { ContributionForm } from "@/components/ideas/contribution-form";
import { ContributionsList } from "@/components/ideas/contributions-list";
import { GainRange } from "@/components/ideas/gain-range";
import { IdeaCard } from "@/components/ideas/idea-card";
import { ImpactGrid } from "@/components/ideas/impact-grid";
import { OriginalContribution } from "@/components/ideas/original-contribution";
import { ReportForm } from "@/components/ideas/report-form";
import { RightOfReply } from "@/components/ideas/right-of-reply";
import { ScoreBreakdown } from "@/components/ideas/score-breakdown";
import { SourcesList } from "@/components/ideas/sources-list";
import { SubmissionOutcome } from "@/components/tracking/submission-outcome";
import { TrackingActions } from "@/components/tracking/tracking-actions";
import { TrackingStatus } from "@/components/tracking/tracking-status";

import { audit } from "./axe";
import {
  dashboardStatsFixture,
  escalationsFixture,
  hiddenIdeasFixture,
  reportsFixture,
  reviewRequestsFixture,
  screeningDecisionsFixture,
  thresholdSettingsFixture,
} from "./fixtures/admin";
import {
  analysisFixture,
  contributionsFixture,
  ideaSummaryFixture,
  rightOfReplyFixture,
  scoreBreakdownFixture,
} from "./fixtures/ideas";
import { acceptedIdeaResultFixture, rejectedIdeaFixture } from "./fixtures/tracking";
import { renderComponent } from "./render-component";

/**
 * Components not reachable through a route yet (wired later) are audited here, on a page-like
 * skeleton: one main landmark and one level-one heading.
 */
function Page({ children }: { children: ReactNode }) {
  return (
    <main>
      <h1>Page de test</h1>
      {children}
    </main>
  );
}

const sourceIds = new Set(analysisFixture.sources.map((source) => source.id));

describe("components accessibility audit", () => {
  it("idea presentation components have no detectable violation", async () => {
    renderComponent(
      <Page>
        <IdeaCard idea={ideaSummaryFixture} rank={1} headingLevel={2} />
        <OriginalContribution text="Texte original." />
        <AiGenerated aria-label="Analyse">
          <ScoreBreakdown data={scoreBreakdownFixture} />
          <GainRange range={analysisFixture.annual_gain_eur} label="Gain annuel estimé" />
          <ProsCons
            pros={analysisFixture.pros}
            cons={analysisFixture.cons}
            risks={analysisFixture.risks}
            sourceIds={sourceIds}
          />
          <ImpactGrid impacts={analysisFixture.impacts} sourceIds={sourceIds} />
          <AssumptionsList assumptions={analysisFixture.assumptions} sourceIds={sourceIds} />
          <SourcesList sources={analysisFixture.sources} />
          <ContributionsList contributions={contributionsFixture} />
        </AiGenerated>
        <RightOfReply reply={rightOfReplyFixture} />
      </Page>,
    );
    expect(await audit()).toEqual([]);
  });

  it("contribution and report forms have no detectable violation", async () => {
    renderComponent(
      <Page>
        <ContributionForm onSubmit={vi.fn()} altchaTestMode />
        <ReportForm onSubmit={vi.fn()} altchaTestMode />
      </Page>,
    );
    await screen.findAllByText(/Je ne suis pas un robot|Chargement de la vérification/);
    expect(await audit()).toEqual([]);
  });

  it("tracking components have no detectable violation", async () => {
    renderComponent(
      <Page>
        <TrackingStatus item={rejectedIdeaFixture} />
        <TrackingActions item={rejectedIdeaFixture} onDelete={vi.fn()} onRequestReview={vi.fn()} />
        <SubmissionOutcome kind="idea" result={acceptedIdeaResultFixture} linkLabel="Idée" />
      </Page>,
    );
    expect(await audit()).toEqual([]);
  });

  it("administration components have no detectable violation", async () => {
    renderComponent(
      <Page>
        <DashboardTiles stats={dashboardStatsFixture} />
        <HiddenIdeasTable rows={hiddenIdeasFixture} onRepublish={vi.fn()} onReanalyze={vi.fn()} />
        <ReportsTable rows={reportsFixture} />
        <EscalationsTable
          rows={escalationsFixture}
          onMarkTransmitted={vi.fn()}
          onDismiss={vi.fn()}
        />
        <ReviewRequestsTable rows={reviewRequestsFixture} onUphold={vi.fn()} onOverturn={vi.fn()} />
        <ScreeningDecisions decisions={screeningDecisionsFixture} onLabel={vi.fn()} />
        <ThresholdSettingsForm initial={thresholdSettingsFixture} onSubmit={vi.fn()} />
        <RightOfReplyEditor
          idea={{ slug: "exemple", title: "Exemple" }}
          initial={{ author_label: "la mairie", body: "Réponse.", requested_on: "2026-10-18" }}
          onSubmit={vi.fn()}
        />
      </Page>,
    );
    expect(await audit()).toEqual([]);
  });
});
