import { Loader2Icon } from "lucide-react";
import { type ReactNode, useState } from "react";

import { errorMessage } from "@/api/errors";
import { Button } from "@/components/ui/button";

interface AsyncActionButtonProps {
  /** Runs the action; a rejection is shown under the button. */
  onAction: () => Promise<void>;
  children: ReactNode;
  variant?: "default" | "outline" | "secondary" | "destructive" | "ghost";
  /** Accessible name when the visible label is ambiguous in a table row. */
  "aria-label"?: string;
}

/** Admin action with its own pending and error states (one per table row). */
export function AsyncActionButton({
  onAction,
  children,
  variant = "outline",
  "aria-label": ariaLabel,
}: AsyncActionButtonProps) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    setPending(true);
    setError(null);
    try {
      await onAction();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(false);
    }
  };

  return (
    <span className="inline-flex flex-col gap-1">
      <Button
        type="button"
        size="sm"
        variant={variant}
        disabled={pending}
        aria-label={ariaLabel}
        onClick={() => {
          void run();
        }}
      >
        {pending ? <Loader2Icon className="animate-spin" aria-hidden="true" /> : null}
        {children}
      </Button>
      {error ? (
        <span role="alert" className="text-xs font-medium text-destructive">
          {error}
        </span>
      ) : null}
    </span>
  );
}
