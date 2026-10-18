import { CheckIcon, CopyIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type CopyState = "idle" | "copied" | "failed";

interface CopyButtonProps {
  /** Text put in the clipboard. */
  value: string;
  /** Visible label, e.g. "Copier le lien". */
  label: string;
  variant?: "default" | "outline" | "secondary";
  size?: "default" | "sm" | "lg";
  className?: string;
}

/**
 * Copies a text to the clipboard and announces the result politely (screen readers). When the
 * Clipboard API is unavailable or refused (insecure context, permissions), invites the visitor
 * to copy the text by hand instead of failing silently.
 */
export function CopyButton({
  value,
  label,
  variant = "outline",
  size = "default",
  className,
}: CopyButtonProps) {
  const [state, setState] = useState<CopyState>("idle");

  useEffect(() => {
    if (state !== "copied") return;
    const timer = window.setTimeout(() => {
      setState("idle");
    }, 4000);
    return () => {
      window.clearTimeout(timer);
    };
  }, [state]);

  const copy = async () => {
    try {
      // The Clipboard API is missing in insecure contexts and some embedded browsers.
      const { clipboard } = navigator as Partial<Navigator>;
      if (!clipboard) throw new Error("Clipboard API unavailable");
      await clipboard.writeText(value);
      setState("copied");
    } catch {
      setState("failed");
    }
  };

  return (
    <div className={cn("flex flex-col gap-1", className)}>
      <Button
        type="button"
        variant={variant}
        size={size}
        onClick={() => {
          void copy();
        }}
      >
        {state === "copied" ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
        {state === "copied" ? "Copié" : label}
      </Button>
      <p role="status" className="text-sm text-muted-foreground empty:hidden">
        {state === "copied" ? "Copié dans le presse-papiers." : null}
        {state === "failed"
          ? "La copie automatique a échoué : sélectionnez le texte et copiez-le manuellement."
          : null}
      </p>
    </div>
  );
}
