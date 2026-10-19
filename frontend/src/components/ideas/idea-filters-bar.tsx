import { SearchIcon, XIcon } from "lucide-react";
import { type SubmitEvent, useId } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useIdeaFilters } from "@/hooks/use-idea-filters";
import {
  defaultIdeaFilters,
  hasActiveIdeaFilters,
  IDEA_SEARCH_MAX_LENGTH,
  IDEA_TERRITORY_MAX_LENGTH,
} from "@/lib/idea-filters";
import type { CategoryRef } from "@/lib/idea-model";
import { useTaxonomyLabels } from "@/hooks/use-taxonomy";
import { adminLevelLabels, entriesOf, ideaKindLabels, ideaSortLabels, isKeyOf } from "@/lib/labels";
import { labelledOptions } from "@/lib/taxonomy-labels";

/** Radix Select reserves the empty string, so "no filter" uses this sentinel value. */
const ALL = "all";

interface IdeaFiltersBarProps {
  /** Categories of the taxonomy (GET /api/taxonomy); the category filter is hidden without. */
  categories?: readonly CategoryRef[];
}

/** Sort and filters of the ranking (AGENTS.md §13.2), kept in the URL query string. */
export function IdeaFiltersBar({ categories = [] }: IdeaFiltersBarProps) {
  const { filters, update } = useIdeaFilters();
  const labels = useTaxonomyLabels();
  const id = useId();

  const onSearch = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const q = data.get("q");
    const territory = data.get("territory");
    update({
      q: typeof q === "string" ? q : "",
      territory: typeof territory === "string" ? territory : "",
    });
  };

  return (
    <div className="grid gap-4 rounded-lg border bg-card p-4 sm:grid-cols-2 lg:grid-cols-4 lg:items-end">
      <form
        role="search"
        aria-label="Rechercher une idée"
        onSubmit={onSearch}
        className="grid gap-4 sm:col-span-2 sm:grid-cols-[2fr_1fr_auto] sm:items-end lg:col-span-4"
        // Re-initialise the fields when the URL changes (reset, back button).
        key={`${filters.q}|${filters.territory}`}
      >
        <div className="space-y-2">
          <Label htmlFor={`${id}-q`}>Rechercher</Label>
          <Input
            id={`${id}-q`}
            name="q"
            type="search"
            defaultValue={filters.q}
            maxLength={IDEA_SEARCH_MAX_LENGTH}
            placeholder="Ex. : transport, énergie, fraude…"
            autoComplete="off"
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`${id}-territory`}>Territoire</Label>
          <Input
            id={`${id}-territory`}
            name="territory"
            defaultValue={filters.territory}
            maxLength={IDEA_TERRITORY_MAX_LENGTH}
            placeholder="Nom ou code INSEE"
            autoComplete="off"
          />
        </div>
        <Button type="submit" variant="secondary">
          <SearchIcon aria-hidden="true" />
          Lancer la recherche
        </Button>
      </form>

      <div className="space-y-2">
        <Label htmlFor={`${id}-sort`}>Trier par</Label>
        <Select
          value={filters.sort}
          onValueChange={(value) => {
            if (isKeyOf(ideaSortLabels, value)) update({ sort: value });
          }}
        >
          <SelectTrigger id={`${id}-sort`} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {entriesOf(ideaSortLabels).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`${id}-level`}>Niveau administratif</Label>
        <Select
          value={filters.adminLevel ?? ALL}
          onValueChange={(value) => {
            update({ adminLevel: isKeyOf(adminLevelLabels, value) ? value : null });
          }}
        >
          <SelectTrigger id={`${id}-level`} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL}>Tous les niveaux</SelectItem>
            {labelledOptions(adminLevelLabels, labels.adminLevels).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`${id}-kind`}>Type de mesure</Label>
        <Select
          value={filters.kind ?? ALL}
          onValueChange={(value) => {
            update({ kind: isKeyOf(ideaKindLabels, value) ? value : null });
          }}
        >
          <SelectTrigger id={`${id}-kind`} className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL}>Tous les types</SelectItem>
            {labelledOptions(ideaKindLabels, labels.ideaKinds).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {categories.length > 0 ? (
        <div className="space-y-2">
          <Label htmlFor={`${id}-category`}>Catégorie</Label>
          <Select
            value={filters.category ?? ALL}
            onValueChange={(value) => {
              update({
                category: categories.some((category) => category.key === value) ? value : null,
              });
            }}
          >
            <SelectTrigger id={`${id}-category`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL}>Toutes les catégories</SelectItem>
              {categories.map((category) => (
                <SelectItem key={category.key} value={category.key}>
                  {category.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      ) : null}

      {hasActiveIdeaFilters(filters) ? (
        <div className="sm:col-span-2 lg:col-span-4">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              update(defaultIdeaFilters);
            }}
          >
            <XIcon aria-hidden="true" />
            Réinitialiser les filtres
          </Button>
        </div>
      ) : null}
    </div>
  );
}
