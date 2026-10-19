import { Loader2Icon } from "lucide-react";
import { type ReactNode, type SubmitEvent, useId, useState } from "react";
import { Link } from "react-router";

import { errorMessage } from "@/api/errors";
import type { AdminIdeaRef } from "@/api/types";
import { paths } from "@/app/paths";
import { Pagination } from "@/components/common/pagination";
import { ErrorState, ListSkeleton } from "@/components/common/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { useDocumentTitle } from "@/hooks/use-document-title";
import { cn } from "@/lib/utils";

import { adminLinkClass } from "./admin-styles";

/** The only `<h1>` of an admin page, with the tab title. */
export function AdminPageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description: ReactNode;
  actions?: ReactNode;
}) {
  useDocumentTitle(`${title} · Administration`);
  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold">{title}</h1>
        <div className="text-muted-foreground">{description}</div>
      </div>
      {actions ? <div className="flex flex-wrap gap-2">{actions}</div> : null}
    </div>
  );
}

interface AdminTableProps {
  caption: string;
  headers: readonly string[];
  empty: string;
  rowCount: number;
  children: ReactNode;
  /** Minimum width before the table scrolls horizontally. */
  minWidth?: string;
}

/** Captioned table, scrollable on small screens, with an empty state. */
export function AdminTable({
  caption,
  headers,
  empty,
  rowCount,
  children,
  minWidth = "min-w-[44rem]",
}: AdminTableProps) {
  return (
    <div className="overflow-x-auto rounded-lg border bg-card">
      <table className={cn("w-full border-collapse text-sm", minWidth)}>
        <caption className="border-b bg-muted/40 px-4 py-3 text-left font-semibold">
          {caption}
        </caption>
        <thead>
          <tr className="border-b text-left text-muted-foreground">
            {headers.map((header) => (
              <th key={header} scope="col" className="px-4 py-2 font-medium">
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rowCount === 0 ? (
            <tr>
              <td colSpan={headers.length} className="px-4 py-6 text-center text-muted-foreground">
                {empty}
              </td>
            </tr>
          ) : (
            children
          )}
        </tbody>
      </table>
    </div>
  );
}

interface QueryLike<T> {
  data: T | undefined;
  error: unknown;
  isPending: boolean;
  isError: boolean;
  refetch: () => unknown;
}

/** Loading and error states of an admin query; renders `children` with the data. */
export function AdminQuery<T>({
  query,
  label,
  children,
}: {
  query: QueryLike<T>;
  label: string;
  children: (data: T) => ReactNode;
}) {
  if (query.isPending) return <ListSkeleton rows={2} label={`Chargement : ${label}…`} />;
  if (query.isError || query.data === undefined) {
    return (
      <ErrorState
        title={`Impossible de charger : ${label}`}
        message={errorMessage(query.error)}
        onRetry={() => {
          void query.refetch();
        }}
      />
    );
  }
  return <>{children(query.data)}</>;
}

export function AdminPagination({
  page,
  totalPages,
  hrefForPage,
}: {
  page: number;
  totalPages: number;
  hrefForPage: (page: number) => string;
}) {
  return <Pagination page={page} pageCount={totalPages} hrefForPage={hrefForPage} />;
}

interface FilterSelectProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: readonly (readonly [string, string])[];
  /** Label of the "no filter" option. */
  all: string;
}

/** Native select of an admin filter (empty value: no filter). */
export function FilterSelect({ label, value, onChange, options, all }: FilterSelectProps) {
  const id = useId();
  return (
    <div className="space-y-1">
      <Label htmlFor={id} className="text-xs text-muted-foreground">
        {label}
      </Label>
      <select
        id={id}
        value={value}
        onChange={(event) => {
          onChange(event.target.value);
        }}
        className="h-9 w-full min-w-40 rounded-md border border-input bg-background px-3 text-sm shadow-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      >
        <option value="">{all}</option>
        {options.map(([key, text]) => (
          <option key={key} value={key}>
            {text}
          </option>
        ))}
      </select>
    </div>
  );
}

/**
 * Link to the public page of an idea or its identifier. An idea withdrawn by its author (owner
 * decision D2) has no public page any more: it is named without a link and marked as such.
 */
export function AdminIdeaLink({
  idea,
  markWithdrawn = true,
}: {
  idea: Pick<AdminIdeaRef, "slug" | "title" | "id"> & { status?: string };
  /** Adds the « Retirée par son auteur » badge (off where the status is already shown). */
  markWithdrawn?: boolean;
}) {
  const title = idea.title ?? "Idée sans titre (pas encore analysée)";
  const withdrawn = idea.status === "withdrawn";
  return (
    <span className="space-y-0.5">
      {idea.slug && !withdrawn ? (
        <Link to={paths.idea(idea.slug)} className={adminLinkClass}>
          {title}
        </Link>
      ) : (
        <span className="font-medium">{title}</span>
      )}
      {withdrawn && markWithdrawn ? (
        <span className="block">
          <Badge variant="outline">Retirée par son auteur</Badge>
        </span>
      ) : null}
      <span className="block font-mono text-xs text-muted-foreground">{idea.id}</span>
    </span>
  );
}

interface ConfirmDialogProps {
  /** Text of the button opening the dialog. */
  trigger: ReactNode;
  triggerLabel?: string;
  triggerVariant?: "default" | "outline" | "secondary" | "destructive" | "ghost";
  title: string;
  description: ReactNode;
  confirm: string;
  /** Free text field (reason or note). */
  field?: { label: string; required: boolean; maxLength: number; help?: string };
  onConfirm: (text: string | null) => Promise<void>;
}

/**
 * Admin action confirmed in a dialog, optionally with a reason or note (kept in the journal,
 * never shown to authors). Errors of the API stay in the dialog.
 */
export function ConfirmDialog({
  trigger,
  triggerLabel,
  triggerVariant = "outline",
  title,
  description,
  confirm,
  field,
  onConfirm,
}: ConfirmDialogProps) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const submit = async (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = text.trim();
    if (field?.required && value === "") {
      setError(`${field.label} : ce champ est obligatoire.`);
      return;
    }
    setPending(true);
    setError(null);
    try {
      await onConfirm(value === "" ? null : value);
      setOpen(false);
      setText("");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setError(null);
      }}
    >
      <DialogTrigger asChild>
        <Button type="button" size="sm" variant={triggerVariant} aria-label={triggerLabel}>
          {trigger}
        </Button>
      </DialogTrigger>
      <DialogContent>
        <form
          onSubmit={(event) => {
            void submit(event);
          }}
          noValidate
          className="space-y-4"
        >
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{description}</DialogDescription>
          </DialogHeader>
          {field ? (
            <div className="space-y-2">
              <Label htmlFor={`${id}-text`}>
                {field.label}
                {field.required ? null : " (facultatif)"}
              </Label>
              <Textarea
                id={`${id}-text`}
                value={text}
                maxLength={field.maxLength}
                rows={4}
                onChange={(event) => {
                  setText(event.target.value);
                }}
                aria-describedby={field.help ? `${id}-help` : undefined}
              />
              {field.help ? (
                <p id={`${id}-help`} className="text-xs text-muted-foreground">
                  {field.help}
                </p>
              ) : null}
            </div>
          ) : null}
          {error ? (
            <p role="alert" className="text-sm font-medium text-destructive">
              {error}
            </p>
          ) : null}
          <DialogFooter>
            <Button type="submit" disabled={pending}>
              {pending ? <Loader2Icon className="animate-spin" aria-hidden="true" /> : null}
              {confirm}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
