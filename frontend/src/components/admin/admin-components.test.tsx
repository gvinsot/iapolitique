import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ApiError } from "@/api/errors";
import type { AdminThresholdsUpdate } from "@/api/types";
import {
  expectedFromLabel,
  hoursUntil,
  parisToday,
  rightOfReplyDeadline,
  thresholdOverrides,
  validateDedupThreshold,
  validateThresholds,
} from "@/lib/admin-model";
import {
  dashboardFixture,
  escalationsFixture,
  ideasFixture,
  jobsFixture,
  reportsFixture,
  reviewRequestsFixture,
  screeningsFixture,
  settingsFixture,
} from "@/test/fixtures/admin";
import { renderComponent } from "@/test/render-component";

import { JobsTable } from "./admin-lists";
import { DashboardQueues, DashboardTiles } from "./dashboard-tiles";
import {
  EscalationsTable,
  IdeasTable,
  ReportsTable,
  ReviewRequestsTable,
} from "./moderation-tables";
import { RightOfReplyEditor } from "./right-of-reply-editor";
import { ScreeningDecisions } from "./screening-decisions";
import { DedupThresholdForm, ThresholdSettingsForm } from "./threshold-settings-form";

describe("admin model", () => {
  it("computes deadlines in hours and days", () => {
    expect(hoursUntil("2026-10-19T08:00:00Z", new Date("2026-10-19T02:00:00Z"))).toBe(6);
    expect(rightOfReplyDeadline("2026-10-30")).toBe("2026-11-02");
    expect(parisToday(new Date("2026-10-19T22:30:00Z"))).toBe("2026-10-20");
  });

  it("deduces the expected decision of a label", () => {
    expect(expectedFromLabel({ final_decision: "reject" }, "correct")).toBe("reject");
    expect(expectedFromLabel({ final_decision: "reject" }, "incorrect")).toBe("accept");
    expect(expectedFromLabel({ final_decision: null }, "incorrect")).toBeNull();
  });

  it("validates thresholds", () => {
    expect(validateThresholds(0.3, 0.7)).toBeNull();
    expect(validateThresholds(0.8, 0.7)).toMatch(/inférieur ou égal/);
    expect(validateThresholds(-1, 0.7)).toMatch(/entre 0 et 1/);
    expect(validateDedupThreshold(0.4, 0.5)).toMatch(/entre 0,5 et 1/);
    expect(validateDedupThreshold(0.9, 0.5)).toBeNull();
  });

  it("sends only the thresholds that differ from the file", () => {
    const update = thresholdOverrides(settingsFixture.screening, (_kind, criterion) =>
      criterion.key === "on_topic"
        ? { reject_below: 0.3, accept_above: 0.8 }
        : { reject_below: criterion.default_reject_below, accept_above: 0.6 },
    );
    expect(update).toEqual({ idea: { on_topic: { accept_above: 0.8 } } });
  });
});

describe("dashboard", () => {
  it("shows the queue, the acceptance rate and the AI costs against the budget", () => {
    renderComponent(<DashboardTiles dashboard={dashboardFixture} />);
    expect(screen.getByText("Jobs en file").closest("li")).toHaveTextContent("12");
    expect(screen.getByText("Jobs en file").closest("li")).toHaveTextContent("1 abandonné");
    expect(screen.getByText("Idées acceptées (30 j)").closest("li")).toHaveTextContent("68 %");
    expect(screen.getByText("Coût IA du jour").closest("li")).toHaveTextContent(
      /50 % du plafond de 5,00 €/,
    );
  });

  it("warns when the daily budget is reached", () => {
    renderComponent(
      <DashboardTiles
        dashboard={{
          ...dashboardFixture,
          ai_costs: { ...dashboardFixture.ai_costs, today_eur: 5, budget_reached: true },
        }}
      />,
    );
    expect(screen.getByText(/les jobs IA attendent le/)).toBeInTheDocument();
  });

  it("links every waiting queue to its page", () => {
    renderComponent(<DashboardQueues queues={dashboardFixture.queues} />);
    expect(screen.getByRole("link", { name: "Jobs abandonnés" })).toHaveAttribute(
      "href",
      "/admin/jobs?status=dead",
    );
    expect(screen.getByRole("link", { name: /Escalades à traiter/ })).toHaveAttribute(
      "href",
      "/admin/escalades",
    );
  });
});

describe("moderation tables", () => {
  it("hides a published idea with a reason and relaunches an analysis", async () => {
    const onHide = vi.fn(() => Promise.resolve());
    const onReanalyze = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <IdeasTable
        rows={ideasFixture}
        onHide={onHide}
        onRepublish={vi.fn()}
        onReanalyze={onReanalyze}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Masquer « Une idée publiée »" }));
    const dialog = await screen.findByRole("dialog", { name: "Masquer cette idée" });
    await user.click(within(dialog).getByRole("button", { name: "Masquer l’idée" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("ce champ est obligatoire");
    await user.type(within(dialog).getByRole("textbox", { name: "Motif" }), "Contenu trompeur");
    await user.click(within(dialog).getByRole("button", { name: "Masquer l’idée" }));
    expect(onHide).toHaveBeenCalledWith(ideasFixture[1]?.id, "Contenu trompeur");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });

    await user.click(
      screen.getByRole("button", {
        name: "Relancer l’analyse de « Créer une taxe sur un exemple »",
      }),
    );
    expect(onReanalyze).toHaveBeenCalledWith(ideasFixture[0]?.id);
    expect(
      screen.getByRole("link", { name: "Droits de réponse de « Créer une taxe sur un exemple »" }),
    ).toHaveAttribute("href", `/admin/idees/${ideasFixture[0]?.id}/reponses`);
  });

  it("keeps the dialog open with the API message when an action fails", async () => {
    const onRepublish = vi.fn(() =>
      Promise.reject(
        new ApiError({
          status: 409,
          code: "idea_not_hidden",
          message: "L’idée n’est pas masquée.",
        }),
      ),
    );
    const { user } = renderComponent(
      <IdeasTable
        rows={ideasFixture}
        onHide={vi.fn()}
        onRepublish={onRepublish}
        onReanalyze={vi.fn()}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: "Republier « Créer une taxe sur un exemple »" }),
    );
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByRole("textbox", { name: "Motif" }), "Erreur de revue");
    await user.click(within(dialog).getByRole("button", { name: "Republier l’idée" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("L’idée n’est pas masquée.");
  });

  it("shows the reports with the reporter contact and the AI review", () => {
    renderComponent(<ReportsTable rows={reportsFixture} />);
    const row = screen.getAllByRole("row")[1];
    expect(row).toHaveTextContent("Source erronée ou absente");
    expect(row).toHaveTextContent("Masquée");
    expect(row).toHaveTextContent("Escaladé");
    expect(screen.getByRole("link", { name: "dominique@example.org" })).toHaveAttribute(
      "href",
      "mailto:dominique@example.org",
    );
  });

  it("never links a reporter address that carries mailto header fields (INJ-7)", () => {
    const hostile = "contact@mairie.fr?cc=attacker%40evil.example&subject=Re%3A";
    const [report] = reportsFixture;
    if (!report) throw new Error("missing fixture");
    renderComponent(<ReportsTable rows={[{ ...report, reporter_email: hostile }]} />);
    const row = screen.getAllByRole("row")[1];
    if (!row) throw new Error("missing row");
    expect(row).toHaveTextContent(hostile);
    expect(row).toHaveTextContent("adresse non conforme");
    expect(within(row).queryByRole("link", { name: hostile })).toBeNull();
    expect(row.querySelector('a[href^="mailto:"]')).toBeNull();
  });

  it("tracks the 24-hour target of an escalation and records its outcome", async () => {
    const onHandle = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <EscalationsTable
        rows={escalationsFixture}
        onHandle={onHandle}
        now={new Date("2026-10-18T20:30:00Z")}
      />,
    );
    expect(screen.getByText("11 h restantes")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /plateforme PHAROS/ })).toHaveAttribute(
      "target",
      "_blank",
    );
    await user.click(screen.getByRole("button", { name: /comme transmis à PHAROS/ }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByRole("textbox"), "Signalement n° 123");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la transmission" }));
    expect(onHandle).toHaveBeenCalledWith("e-1", "transmitted", "Signalement n° 123");
  });

  it("flags an overdue escalation", () => {
    renderComponent(
      <EscalationsTable
        rows={escalationsFixture}
        onHandle={vi.fn()}
        now={new Date("2026-10-19T09:00:00Z")}
      />,
    );
    expect(screen.getByText("Hors délai de 24 h")).toBeInTheDocument();
  });

  it("resolves a human review request without a note", async () => {
    const onResolve = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <ReviewRequestsTable rows={reviewRequestsFixture} onResolve={onResolve} />,
    );
    expect(screen.getByText("Refus par le filtrage")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Infirmer la décision/ }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Infirmer la décision" }));
    expect(onResolve).toHaveBeenCalledWith("rr-1", "overturned", null);
  });
});

describe("ScreeningDecisions", () => {
  it("shows per-criterion probabilities and labels a decided screening", async () => {
    const onLabel = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <ScreeningDecisions screenings={screeningsFixture} onLabel={onLabel} onUnlabel={vi.fn()} />,
    );
    const [decided, failed] = screen
      .getAllByRole("listitem", { name: "" })
      .filter((item) => item.matches("ul[aria-label='Décisions de filtrage'] > li"));
    expect(decided).toHaveTextContent("on_topic");
    expect(decided).toHaveTextContent("0,12 · échec");
    await user.click(within(decided!).getByRole("button", { name: "Décision juste" }));
    expect(onLabel).toHaveBeenCalledWith("s-1", { label: "correct" });

    // Without a decision (service error), the expected decision is asked for.
    expect(failed).toHaveTextContent("Erreur : timeout");
    await user.click(within(failed!).getByRole("button", { name: "Aurait dû être acceptée" }));
    expect(onLabel).toHaveBeenCalledWith("s-2", { label: "incorrect", expected: "accept" });
  });

  it("shows the API message when a label is refused", async () => {
    const onLabel = vi.fn(() =>
      Promise.reject(
        new ApiError({ status: 422, code: "label_inconsistent", message: "Incohérent." }),
      ),
    );
    const { user } = renderComponent(
      <ScreeningDecisions screenings={screeningsFixture} onLabel={onLabel} onUnlabel={vi.fn()} />,
    );
    await user.click(screen.getAllByRole("button", { name: "Décision erronée" })[0]!);
    expect(await screen.findByRole("alert")).toHaveTextContent("Incohérent.");
  });
});

describe("settings forms", () => {
  it("validates and saves only the overridden thresholds", async () => {
    const onSave = vi.fn<(update: AdminThresholdsUpdate) => Promise<void>>(() => Promise.resolve());
    const { user } = renderComponent(
      <ThresholdSettingsForm settings={settingsFixture} onSave={onSave} />,
    );
    const accept = screen.getAllByLabelText("Acceptation à partir de")[1]!;
    await user.clear(accept);
    await user.type(accept, "0,1");
    await user.click(screen.getByRole("button", { name: "Enregistrer les seuils" }));
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(/inférieur ou égal/);

    await user.clear(accept);
    await user.type(accept, "0,65");
    await user.click(screen.getByRole("button", { name: "Enregistrer les seuils" }));
    expect(onSave).toHaveBeenCalledWith({
      idea: { on_topic: { accept_above: 0.75 }, realism: { accept_above: 0.65 } },
    });
    expect(await screen.findByText("Seuils de filtrage enregistrés.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Revenir aux seuils du fichier" }));
    expect(onSave).toHaveBeenLastCalledWith({});
  });

  it("checks the duplicate threshold against the API minimum", async () => {
    const onSave = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <DedupThresholdForm settings={settingsFixture} onSave={onSave} />,
    );
    const field = screen.getByLabelText("Seuil de similarité");
    await user.clear(field);
    await user.type(field, "0,3");
    await user.click(screen.getByRole("button", { name: "Enregistrer le seuil" }));
    expect(screen.getByRole("alert")).toHaveTextContent("entre 0,5 et 1");
    await user.clear(field);
    await user.type(field, "0,92");
    await user.click(screen.getByRole("button", { name: "Enregistrer le seuil" }));
    expect(onSave).toHaveBeenCalledWith(0.92);
  });
});

describe("JobsTable", () => {
  it("retries a dead job", async () => {
    const onRetry = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(<JobsTable rows={jobsFixture} onRetry={onRetry} />);
    expect(screen.getByText("Analyse")).toBeInTheDocument();
    expect(screen.getByText("Abandonné")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Relancer le job n° 42" }));
    expect(onRetry).toHaveBeenCalledWith(42);
  });
});

describe("RightOfReplyEditor", () => {
  it("requires the fields, shows the 3-day deadline and previews the reply", async () => {
    const onSubmit = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <RightOfReplyEditor
        ideaTitle="Mutualiser les achats des communes"
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

  it("refuses a request date in the future", async () => {
    const onSubmit = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <RightOfReplyEditor
        ideaTitle="Exemple"
        initial={{ author_label: "la mairie", body: "Réponse.", requested_on: "2026-10-25" }}
        onSubmit={onSubmit}
        today={new Date("2026-10-20T10:00:00Z")}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Publier la réponse" }));
    expect(screen.getByText(/ne peut pas être dans le futur/)).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
