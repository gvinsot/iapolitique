/**
 * Loaded on demand by AltchaField: registers the <altcha-widget> element. The `external` build
 * keeps styles and the proof-of-work worker in separate files served from our own origin, as
 * required by the Content-Security-Policy of nginx.conf (no inline worker, no third party).
 */
import "altcha/external";
import "altcha/altcha.css";

export { default as altchaWorkerUrl } from "altcha/worker?url";
