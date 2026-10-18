import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ApiError } from "@/api/errors";
import {
  acceptanceRate,
  escalationHoursLeft,
  rightOfReplyDeadline,
  type ThresholdSettings,
  validateDedupThreshold,
  validateThreshold,
} from "@/lib/admin-model";
import {
  dashboardStatsFixture,
  escalationsFixture,
  hiddenIdeasFixture,
  reportsFixture,
  reviewRequestsFixture,
  screeningDecisionsFixture,
  thresholdSettingsFixture,
} from "@/test/fixtures/admin";
import { renderComponent } from "@/test/render-component";

import { DashboardTiles } from "./dashboard-tiles";
import {
  EscalationsTable,
  HiddenIdeasTable,
  ReportsTable,
  ReviewRequestsTable,
} from "./moderation-tables";
import { RightOfReplyEditor } from "./right-of-reply-editor";
import { ScreeningDecisions } from "./screening-decisions";
import { ThresholdSettingsForm } from "./threshold-settings-form";

const plain = (value: string | null) => (value ?? "").replace(/\s/g, " ");

describe("admin model", () => {
  it("computes the acceptance rate without the gray zone", () => {
    expect(acceptanceRate(dashboardStatsFixture.screening)).toBe(0.75);
    expect(acceptanceRate({ window_days: 7, accepted: 0, rejected: 0, gray_zone: 3 })).toBeNull();
  });

  it("computes the escalation and right of reply deadlines", () => {
    expect(escalationHoursLeft("2026-10-18T08:00:00Z", new Date("2026-10-18T20:00:00Z"))).toBe(12);
    expect(escalationHoursLeft("2026-10-18T08:00:00Z", new Date("2026-10-19T10:00:00Z"))).toBe(-2);
    expect(rightOfReplyDeadline("2026-10-30")).toBe("2026-11-02");
  });

  it("validates thresholds", () => {
    expect(validateThreshold({ key: "x", reject_below: 0.3, accept_above: 0.6 })).toBeNull();
    expect(validateThreshold({ key: "x", reject_below: 0.7, accept_above: 0.6 })).toMatch(
      /inférieur ou égal/,
    );
    expect(validateThreshold({ key: "x", reject_below: Number.NaN, accept_above: 1.2 })).toMatch(
      /entre 0 et 1/,
    );
    expect(validateDedupThreshold(0.95)).toBeNull();
    expect(validateDedupThreshold(0.2)).not.toBeNull();
  });
});

describe("DashboardTiles", () => {
  it("shows the queue, the acceptance rate and the AI costs against the budget", () => {
    renderComponent(<DashboardTiles stats={dashboardStatsFixture} />);
    const tiles = screen.getAllByRole("listitem");
    expect(tiles[0]).toHaveTextContent("Jobs en file12");
    expect(tiles[0]).toHaveTextContent("1 en échec");
    expect(plain(tiles[1]!.textContent)).toContain("Taux d’acceptation (7 j)75 %");
    expect(plain(tiles[2]!.textContent)).toContain("Coût IA du jour2,50 €");
    expect(plain(tiles[2]!.textContent)).toContain("50 % du plafond de 5,00 €");
    expect(plain(tiles[3]!.textContent)).toContain("Coût IA du mois41,24 €");
    expect(tiles[4]).toHaveTextContent("Escalades en attente1");
  });

  it("warns when the daily budget is reached", () => {
    renderComponent(
      <DashboardTiles
        stats={{
          ...dashboardStatsFixture,
          ai_costs: { today_eur: 5.2, month_eur: 60, daily_budget_eur: 5 },
        }}
      />,
    );
    expect(screen.getByText(/les jobs attendent minuit/)).toBeInTheDocument();
  });
});

describe("moderation tables", () => {
  it("lists hidden ideas and runs row actions", async () => {
    const onRepublish = vi.fn(() => Promise.resolve());
    const onReanalyze = vi.fn(() =>
      Promise.reject(new ApiError({ status: 409, code: "x", message: "Analyse déjà en cours." })),
    );
    const { user } = renderComponent(
      <HiddenIdeasTable
        rows={hiddenIdeasFixture}
        onRepublish={onRepublish}
        onReanalyze={onReanalyze}
      />,
    );
    const table = screen.getByRole("table", { name: "Idées masquées" });
    expect(within(table).getByText("Revue de signalement (IA)")).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Republier « Créer une taxe sur un exemple »" }),
    );
    expect(onRepublish).toHaveBeenCalledWith("taxe-exemple");
    await user.click(screen.getByRole("button", { name: /Relancer l’analyse/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Analyse déjà en cours.");
  });

  it("shows empty states", () => {
    renderComponent(<ReportsTable rows={[]} />);
    expect(screen.getByText("Aucun signalement.")).toBeInTheDocument();
  });

  it("shows the reports with their AI review and escalation flag", () => {
    renderComponent(<ReportsTable rows={reportsFixture} />);
    const rows = screen.getAllByRole("row");
    expect(rows[1]).toHaveTextContent("Source erronée ou absente");
    expect(rows[1]).toHaveTextContent("Masquée");
    expect(rows[2]).toHaveTextContent("En attente");
    expect(rows[2]).toHaveTextContent("Escaladé");
  });

  it("tracks the 24-hour escalation target and links to PHAROS", async () => {
    const onMarkTransmitted = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <EscalationsTable
        rows={escalationsFixture}
        onMarkTransmitted={onMarkTransmitted}
        onDismiss={vi.fn()}
        now={new Date("2026-10-18T20:30:00Z")}
      />,
    );
    expect(screen.getByRole("link", { name: /PHAROS/ })).toHaveAttribute(
      "href",
      "https://www.internet-signalement.gouv.fr",
    );
    const rows = screen.getAllByRole("row");
    expect(rows[1]).toHaveTextContent("11 h restantes");
    expect(rows[2]).toHaveTextContent("Transmis à PHAROS");
    expect(within(rows[2]!).queryByRole("button")).toBeNull();
    await user.click(screen.getByRole("button", { name: /comme transmis à PHAROS/ }));
    expect(onMarkTransmitted).toHaveBeenCalledWith("e-1");
  });

  it("flags an overdue escalation", () => {
    renderComponent(
      <EscalationsTable
        rows={escalationsFixture}
        onMarkTransmitted={vi.fn()}
        onDismiss={vi.fn()}
        now={new Date("2026-10-20T00:00:00Z")}
      />,
    );
    expect(screen.getByText("Hors délai de 24 h")).toBeInTheDocument();
  });

  it("handles human review requests", async () => {
    const onOverturn = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <ReviewRequestsTable
        rows={reviewRequestsFixture}
        onUphold={vi.fn()}
        onOverturn={onOverturn}
      />,
    );
    expect(
      screen.getByText("Règle 4.5 : Une mesure pour les finances publiques"),
    ).toBeInTheDocument();
    expect(screen.getByText("Mon idée réduit bien une dépense.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Infirmer/ }));
    expect(onOverturn).toHaveBeenCalledWith("rr-1");
  });
});

describe("ScreeningDecisions", () => {
  it("shows per-criterion probabilities with thresholds and labels decisions", async () => {
    const onLabel = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <ScreeningDecisions decisions={screeningDecisionsFixture} onLabel={onLabel} />,
    );
    const cards = screen
      .getAllByRole("listitem")
      .filter((item) => item.parentElement?.tagName === "UL" && item.querySelector("[role=group]"));
    expect(cards).toHaveLength(2);
    const first = cards[0]!;
    expect(first).toHaveTextContent("Refusée");
    expect(plain(first.textContent)).toContain(
      "0,12 · échec (rejet sous 0,3, acceptation au-dessus de 0,5)",
    );
    expect(cards[1]).toHaveTextContent("Texte purgé : seuls les scores sont conservés.");
    expect(within(cards[1]!).getByRole("button", { name: "Décision juste" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    const wrong = within(first).getByRole("button", { name: "Décision erronée" });
    expect(wrong).toHaveAttribute("aria-pressed", "false");
    await user.click(wrong);
    expect(onLabel).toHaveBeenCalledWith("d-1", "incorrect");
    expect(wrong).toHaveAttribute("aria-pressed", "true");
  });
});

describe("ThresholdSettingsForm", () => {
  it("validates and submits the thresholds as numbers", async () => {
    const onSubmit = vi.fn<(settings: ThresholdSettings) => Promise<void>>(() => Promise.resolve());
    const { user } = renderComponent(
      <ThresholdSettingsForm initial={thresholdSettingsFixture} onSubmit={onSubmit} />,
    );
    const realism = screen.getByRole("group", { name: "realism" });
    const reject = within(realism).getByRole("textbox", { name: "Rejet en dessous de" });
    await user.clear(reject);
    await user.type(reject, "0,8");
    await user.click(screen.getByRole("button", { name: "Enregistrer les seuils" }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(within(realism).getByRole("alert")).toHaveTextContent(/inférieur ou égal/);

    await user.clear(reject);
    await user.type(reject, "0,35");
    const dedup = screen.getByRole("textbox", { name: "Seuil de similarité" });
    await user.clear(dedup);
    await user.type(dedup, "0.97");
    await user.click(screen.getByRole("button", { name: "Enregistrer les seuils" }));
    expect(onSubmit).toHaveBeenCalledWith({
      ...thresholdSettingsFixture,
      idea: [
        { key: "respectful", reject_below: 0.3, accept_above: 0.6 },
        { key: "realism", reject_below: 0.35, accept_above: 0.5 },
      ],
      dedup_similarity_threshold: 0.97,
    });
    expect(await screen.findByText("Seuils enregistrés.")).toBeInTheDocument();
  });
});

describe("RightOfReplyEditor", () => {
  const idea = { slug: "mutualiser-achats", title: "Mutualiser les achats des communes" };

  it("requires the fields, shows the 3-day deadline and previews the reply", async () => {
    const onSubmit = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <RightOfReplyEditor
        idea={idea}
        onSubmit={onSubmit}
        today={new Date("2026-10-20T10:00:00Z")}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Publier la réponse" }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(/Indiquez comment la personne/)).toBeInTheDocument();
    expect(screen.getByText("À publier au plus tard le 23 octobre 2026.")).toBeInTheDocument();

    const requested = screen.getByLabelText("Demande reçue le");
    await user.clear(requested);
    await user.type(requested, "2026-10-15");
    expect(screen.getByText(/délai dépassé/)).toBeInTheDocument();

    await user.type(screen.getByRole("textbox", { name: "Auteur de la réponse" }), "la mairie");
    await user.type(
      screen.getByRole("textbox", { name: /Texte de la réponse/ }),
      "Ce dispositif existe déjà.",
    );
    expect(screen.getByRole("region", { name: "Droit de réponse" })).toHaveTextContent(
      "Ce dispositif existe déjà.",
    );
    await user.click(screen.getByRole("button", { name: "Publier la réponse" }));
    expect(onSubmit).toHaveBeenCalledWith({
      author_label: "la mairie",
      body: "Ce dispositif existe déjà.",
      requested_on: "2026-10-15",
    });
    expect(await screen.findByText("Réponse publiée sous l’idée.")).toBeInTheDocument();
  });
});
