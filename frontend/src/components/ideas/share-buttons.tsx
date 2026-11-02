import { Share2Icon } from "lucide-react";
import { useMemo } from "react";

import { CopyButton } from "@/components/common/copy-button";
import { Button } from "@/components/ui/button";
import { shareTargets, shareText } from "@/lib/share";

interface ShareButtonsProps {
  url: string;
  title: string;
}

/**
 * Share links of an idea (plain links, no third-party script: `lib/share.ts`), the browser's
 * own share sheet when available (mobile), and a "copy the link" button.
 */
export function ShareButtons({ url, title }: ShareButtonsProps) {
  const targets = useMemo(() => shareTargets({ url, title }), [url, title]);
  const canShare = typeof navigator !== "undefined" && typeof navigator.share === "function";

  return (
    <nav aria-label="Partager cette idée" className="flex flex-wrap items-start gap-2">
      {canShare ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => {
            // Cancelling the share sheet rejects the promise: nothing to report.
            navigator.share({ title: shareText(title), url }).catch(() => undefined);
          }}
        >
          <Share2Icon aria-hidden="true" />
          Partager
        </Button>
      ) : null}
      <ul className="flex flex-wrap gap-2">
        {targets.map((target) => (
          <li key={target.key}>
            <Button asChild variant="outline" size="sm">
              <a
                href={target.href}
                {...(target.key === "email"
                  ? {}
                  : { target: "_blank", rel: "noopener noreferrer nofollow" })}
              >
                {target.label}
                <span className="sr-only">
                  {target.key === "email"
                    ? " : partager par e-mail"
                    : ` : partager sur ${target.label} (nouvel onglet)`}
                </span>
              </a>
            </Button>
          </li>
        ))}
      </ul>
      <CopyButton value={url} label="Copier le lien" size="sm" />
    </nav>
  );
}
