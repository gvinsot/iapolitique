/**
 * Types of the ALTCHA web component (altcha 2.x ships no types for its `external` build).
 * Only the options and methods used by src/components/forms/altcha-field.tsx are declared.
 */

import type { CSSProperties, Ref } from "react";

export type AltchaState = "unverified" | "verifying" | "verified" | "expired" | "error";

export interface AltchaStateChangeDetail {
  state: AltchaState;
  payload?: string;
}

export interface AltchaWidgetElement extends HTMLElement {
  verify: () => Promise<void>;
  reset: (state?: AltchaState, error?: string | null) => void;
  getState: () => AltchaState;
  configure: (options: { test?: boolean | number }) => void;
}

export interface AltchaWidgetProps {
  ref?: Ref<AltchaWidgetElement>;
  id?: string;
  auto?: "off" | "onfocus" | "onload" | "onsubmit";
  challengeurl?: string;
  credentials?: "omit" | "same-origin" | "include";
  hidefooter?: boolean;
  hidelogo?: boolean;
  name?: string;
  strings?: string;
  workerurl?: string;
  workers?: number;
  style?: CSSProperties;
}

declare module "react" {
  namespace JSX {
    interface IntrinsicElements {
      "altcha-widget": AltchaWidgetProps;
    }
  }
}
