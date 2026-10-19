/**
 * E-mail addresses typed by visitors (report form, AGENTS.md §5.6) and shown to the publisher.
 *
 * The pattern is a strict subset of RFC 5322 `addr-spec`: a dot-atom local part without the
 * characters that change the meaning of a `mailto:` URL (`?` starts hfields such as `cc`, `bcc`
 * or `body`, `&` separates them, `%` decodes into either, `#` starts a fragment; RFC 6068), and a
 * domain made of DNS labels with an alphabetic top-level label (Unicode letters allowed for IDN).
 * Anything else is never turned into a link (finding INJ-7).
 */

export const EMAIL_MAX_LENGTH = 254;

const LOCAL_ATOM = "[A-Za-z0-9!$'*+/=^_`{|}~-]+";
const LOCAL_PART = new RegExp(`^${LOCAL_ATOM}(?:\\.${LOCAL_ATOM})*$`);
const DOMAIN = /^(?:[\p{L}\p{N}](?:[\p{L}\p{N}-]{0,61}[\p{L}\p{N}])?\.)+\p{L}{2,63}$/u;

/** True when `value` is an address the site accepts and may put in a `mailto:` link. */
export function isValidEmail(value: string): boolean {
  if (value.length > EMAIL_MAX_LENGTH) return false;
  const at = value.lastIndexOf("@");
  if (at <= 0) return false;
  const local = value.slice(0, at);
  const domain = value.slice(at + 1);
  return local.length <= 64 && LOCAL_PART.test(local) && DOMAIN.test(domain);
}

/** `mailto:` URL with no header fields for a valid address, `null` otherwise (show it as text). */
export function mailtoHref(value: string | null | undefined): string | null {
  if (!value || !isValidEmail(value)) return null;
  return `mailto:${value}`;
}
