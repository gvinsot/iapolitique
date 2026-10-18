import { Loader2Icon, ShieldCheckIcon } from "lucide-react";
import { type Ref, useEffect, useId, useImperativeHandle, useRef, useState } from "react";

import type { AltchaStateChangeDetail, AltchaWidgetElement } from "@/types/altcha";

/** Self-hosted proof-of-work challenge (AGENTS.md §7.2, §13.2). */
export const ALTCHA_CHALLENGE_URL = "/api/altcha/challenge";

/** French texts of the widget (it is not localised by default). */
const ALTCHA_STRINGS = JSON.stringify({
  ariaLinkLabel: "",
  enterCode: "Saisissez le code",
  enterCodeAria: "Saisissez le code que vous entendez. Appuyez sur Espace pour l’écouter.",
  error: "La vérification a échoué. Réessayez dans quelques instants.",
  expired: "La vérification a expiré. Cochez à nouveau la case.",
  footer: "",
  getAudioChallenge: "Obtenir un code audio",
  label: "Je ne suis pas un robot",
  loading: "Chargement…",
  reload: "Recharger",
  verificationRequired: "Vérification requise.",
  verified: "Vérification réussie",
  verify: "Vérifier",
  verifying: "Vérification en cours…",
  waitAlert: "Vérification en cours, veuillez patienter.",
});

/** Small mock challenge: solved instantly in tests, even without Web Workers. */
const TEST_MODE_MAX_NUMBER = 100;

/** Resolves at the end of an ongoing verification, whatever its outcome. */
function waitUntilSettled(widget: AltchaWidgetElement): Promise<void> {
  return new Promise((resolve) => {
    const onStateChange = (event: Event) => {
      if ((event as CustomEvent<AltchaStateChangeDetail>).detail.state === "verifying") return;
      widget.removeEventListener("statechange", onStateChange);
      resolve();
    };
    widget.addEventListener("statechange", onStateChange);
  });
}

export interface AltchaHandle {
  /**
   * Returns the proof-of-work payload, solving the challenge first if needed. Resolves to null
   * when verification failed (network error, expired challenge).
   */
  solve: () => Promise<string | null>;
  /** Forgets the payload: a payload is accepted only once by the API. */
  reset: () => void;
}

interface AltchaFieldProps {
  ref?: Ref<AltchaHandle>;
  /** Error to show under the widget (set by the form when solving failed). */
  error?: string | null;
  /** Uses the widget's built-in mock verification. Component tests only. */
  testMode?: boolean;
}

/**
 * Anti-robot check with the official ALTCHA web component, fully self-hosted: the challenge
 * comes from our API, the computation runs in a worker served by our origin, no cookie is set
 * (the payload travels in the submission body).
 */
export function AltchaField({ ref, error, testMode = false }: AltchaFieldProps) {
  const id = useId();
  const widgetRef = useRef<AltchaWidgetElement>(null);
  const payloadRef = useRef<string | null>(null);
  const [workerUrl, setWorkerUrl] = useState<string | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);

  useEffect(() => {
    let active = true;
    import("./altcha-loader")
      .then((module) => {
        if (active) setWorkerUrl(module.altchaWorkerUrl);
      })
      .catch((cause: unknown) => {
        console.error(cause);
        if (active) setLoadFailed(true);
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const widget = widgetRef.current;
    if (!widget) return;
    const onStateChange = (event: Event) => {
      const { state, payload } = (event as CustomEvent<AltchaStateChangeDetail>).detail;
      payloadRef.current = state === "verified" && payload ? payload : null;
    };
    widget.addEventListener("statechange", onStateChange);
    return () => {
      widget.removeEventListener("statechange", onStateChange);
    };
  }, [workerUrl]);

  useImperativeHandle(
    ref,
    () => ({
      async solve() {
        if (payloadRef.current) return payloadRef.current;
        const widget = widgetRef.current;
        // Methods exist once the element has mounted (a tick after insertion).
        if (!widget || typeof widget.verify !== "function") return null;
        try {
          if (widget.getState() === "verifying") {
            // Started on focus: wait for it rather than restarting the computation.
            await waitUntilSettled(widget);
          } else {
            // A numeric `test` only takes effect through configure(): attributes are coerced.
            if (testMode) widget.configure({ test: TEST_MODE_MAX_NUMBER });
            await widget.verify();
          }
        } catch {
          return null;
        }
        return payloadRef.current;
      },
      reset() {
        payloadRef.current = null;
        if (typeof widgetRef.current?.reset === "function") widgetRef.current.reset();
      },
    }),
    [testMode],
  );

  const errorId = `${id}-error`;
  return (
    <div className="space-y-2">
      <p className="flex items-center gap-2 text-sm font-medium">
        <ShieldCheckIcon className="size-4 text-muted-foreground" aria-hidden="true" />
        Vérification anti-robot
      </p>
      {workerUrl ? (
        <altcha-widget
          ref={widgetRef}
          id={`${id}-widget`}
          challengeurl={ALTCHA_CHALLENGE_URL}
          workerurl={workerUrl}
          credentials="omit"
          // Starts the computation as soon as the visitor focuses the form (not in tests).
          auto={testMode ? "off" : "onfocus"}
          hidefooter
          hidelogo
          name="altcha"
          strings={ALTCHA_STRINGS}
        />
      ) : loadFailed ? (
        <p className="text-sm text-destructive">
          La vérification anti-robot n’a pas pu se charger. Rechargez la page pour réessayer.
        </p>
      ) : (
        <p className="flex items-center gap-2 text-sm text-muted-foreground" role="status">
          <Loader2Icon className="size-4 animate-spin" aria-hidden="true" />
          Chargement de la vérification…
        </p>
      )}
      <p className="text-xs text-muted-foreground">
        Un petit calcul effectué par votre navigateur, sans cookie ni service tiers.
      </p>
      {error ? (
        <p id={errorId} role="alert" className="text-sm font-medium text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  );
}
