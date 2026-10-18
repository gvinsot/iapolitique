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
} from "@/lib/idea-filters";
import { adminLevelLabels, entriesOf, ideaKindLabels, ideaSortLabels, isKeyOf } from "@/lib/labels";

/** Radix Select reserves the empty string, so "no filter" uses this sentinel value. */
const ALL = "all";

export function IdeaFiltersBar() {
  const { filters, update } = useIdeaFilters();
  const id = useId();

  const onSearch = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = new FormData(event.currentTarget).get("q");
    update({ q: typeof value === "string" ? value : "" });
  };

  return (
    <div className="grid gap-4 rounded-lg border bg-card p-4 sm:grid-cols-2 lg:grid-cols-[2fr_1fr_1fr_1fr] lg:items-end">
      <form
        role="search"
        aria-label="Rechercher une idée"
        onSubmit={onSearch}
        className="space-y-2 sm:col-span-2 lg:col-span-1"
        // Re-initialise the field when the URL changes (reset, back button).
        key={filters.q}
      >
        <Label htmlFor={`${id}-q`}>Rechercher</Label>
        <div className="flex gap-2">
          <Input
            id={`${id}-q`}
            name="q"
            type="search"
            defaultValue={filters.q}
            maxLength={IDEA_SEARCH_MAX_LENGTH}
            placeholder="Ex. : transport, énergie, fraude…"
            autoComplete="off"
          />
          <Button type="submit" variant="secondary" size="icon" aria-label="Lancer la recherche">
            <SearchIcon aria-hidden="true" />
          </Button>
        </div>
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
            {entriesOf(adminLevelLabels).map(([value, label]) => (
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
            {entriesOf(ideaKindLabels).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

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
