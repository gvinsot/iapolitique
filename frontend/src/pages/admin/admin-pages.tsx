import {
  ActivityIcon,
  CoinsIcon,
  FlagIcon,
  type LucideIcon,
  SlidersHorizontalIcon,
  TagsIcon,
} from "lucide-react";

import { EmptyState } from "@/components/common/states";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useDocumentTitle } from "@/hooks/use-document-title";

interface AdminSectionProps {
  title: string;
  description: string;
  icon: LucideIcon;
  upcoming: string;
}

function AdminSection({ title, description, icon, upcoming }: AdminSectionProps) {
  useDocumentTitle(`${title} · Administration`);
  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold">{title}</h1>
        <p className="text-muted-foreground">{description}</p>
      </div>
      <EmptyState icon={icon} title="Bientôt disponible" description={upcoming} />
    </div>
  );
}

const dashboardMetrics = [
  "Jobs en file",
  "Taux d’acceptation (7 j)",
  "Coût IA du jour",
  "Coût IA du mois",
] as const;

export function AdminDashboardPage() {
  useDocumentTitle("Tableau de bord · Administration");
  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold">Tableau de bord</h1>
        <p className="text-muted-foreground">
          Vue d’ensemble de l’activité : file de traitement, filtrage et dépenses IA.
        </p>
      </div>
      <ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {dashboardMetrics.map((label) => (
          <li key={label}>
            <Card className="gap-2 py-4">
              <CardHeader className="px-4">
                <CardTitle className="text-sm font-medium text-muted-foreground">{label}</CardTitle>
              </CardHeader>
              <CardContent className="px-4">
                <span className="sr-only">Non disponible</span>
                <Skeleton className="h-8 w-20" aria-hidden="true" />
              </CardContent>
            </Card>
          </li>
        ))}
      </ul>
      <EmptyState
        icon={ActivityIcon}
        title="Indicateurs bientôt disponibles"
        description="Les compteurs seront alimentés par l’API d’administration."
      />
    </div>
  );
}

export function AdminModerationPage() {
  return (
    <AdminSection
      title="Modération"
      description="Idées masquées et signalements regroupés par idée."
      icon={FlagIcon}
      upcoming="Vous pourrez consulter les signalements et leur réévaluation par IA, masquer ou republier une idée et relancer une analyse."
    />
  );
}

export function AdminDecisionsPage() {
  return (
    <AdminSection
      title="Décisions de filtrage"
      description="Scores bruts du modèle de décision et étiquetage pour calibrer les seuils."
      icon={TagsIcon}
      upcoming="Vous pourrez parcourir les décisions récentes, voir les probabilités par critère et indiquer si chaque décision était juste."
    />
  );
}

export function AdminCostsPage() {
  return (
    <AdminSection
      title="Coûts IA"
      description="Appels IA, consommation et budget journalier."
      icon={CoinsIcon}
      upcoming="Vous pourrez suivre le coût des appels par usage, par modèle et par jour, ainsi que le plafond journalier."
    />
  );
}

export function AdminSettingsPage() {
  return (
    <AdminSection
      title="Réglages"
      description="Seuils de filtrage et de détection des doublons."
      icon={SlidersHorizontalIcon}
      upcoming="Vous pourrez ajuster les seuils de rejet et d’acceptation de chaque critère et le seuil de similarité des doublons."
    />
  );
}
