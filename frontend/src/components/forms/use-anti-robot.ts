import { useRef, useState } from "react";

import type { AltchaHandle } from "./altcha-field";

export const ALTCHA_FAILED_MESSAGE =
  "La vérification anti-robot a échoué. Vérifiez votre connexion puis réessayez.";

/**
 * Shared anti-robot step of the public forms: solves the ALTCHA challenge right before a
 * submission and makes the single-use payload unusable afterwards.
 */
export function useAntiRobot() {
  const altchaRef = useRef<AltchaHandle>(null);
  const [solving, setSolving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** The payload, or null after showing the failure message. */
  const solve = async (): Promise<string | null> => {
    setError(null);
    setSolving(true);
    const payload = (await altchaRef.current?.solve()) ?? null;
    setSolving(false);
    if (!payload) setError(ALTCHA_FAILED_MESSAGE);
    return payload;
  };

  /** Runs the submission with a fresh payload, then discards it whatever the outcome. */
  const submitWith = async (submit: (altcha: string) => void | Promise<void>) => {
    const payload = await solve();
    if (!payload) return;
    try {
      await submit(payload);
    } catch {
      // Shown by the page (submission error state); the form keeps its content for a retry.
    } finally {
      altchaRef.current?.reset();
    }
  };

  return { altchaRef, solving, error, submitWith };
}
