import { cn } from "@/lib/utils";

/** Logo: three ascending bars (ranking by cost-effectiveness). Decorative. */
export function BrandMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 32 32"
      aria-hidden="true"
      focusable="false"
      className={cn("size-8 shrink-0", className)}
    >
      <rect width="32" height="32" rx="7" className="fill-primary" />
      <rect x="7" y="17" width="4.5" height="8" rx="1.2" className="fill-primary-foreground" />
      <rect x="13.75" y="12" width="4.5" height="13" rx="1.2" className="fill-primary-foreground" />
      <rect x="20.5" y="7" width="4.5" height="18" rx="1.2" className="fill-primary-foreground" />
    </svg>
  );
}
