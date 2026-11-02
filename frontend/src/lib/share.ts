/**
 * Share links of an idea page (AGENTS.md §1, §4.2): plain links to each network's public share
 * address, built here from the idea's canonical URL and neutral title. No script, widget, pixel
 * or icon of a network is loaded: nothing leaves the browser until the visitor clicks a link,
 * and then only the page address and title, in the URL the visitor opens.
 */

export interface ShareTarget {
  /** Stable key, used as React key and in tests. */
  key: string;
  /** Visible name of the network. */
  label: string;
  /** Address opened in a new tab (or the mail client for `mailto:`). */
  href: string;
}

export interface SharedPage {
  /** Canonical address of the page (`canonical_url` of the API). */
  url: string;
  /** Neutral title of the idea. */
  title: string;
}

/** Share text: the title followed by the site's name, never anything typed by a visitor. */
export function shareText(title: string): string {
  return `${title} – iapolitique.fr`;
}

/** Share links for the main social networks, in display order. */
export function shareTargets({ url, title }: SharedPage): ShareTarget[] {
  const u = encodeURIComponent(url);
  const text = encodeURIComponent(shareText(title));
  const textAndUrl = encodeURIComponent(`${shareText(title)} ${url}`);
  return [
    {
      key: "facebook",
      label: "Facebook",
      href: `https://www.facebook.com/sharer/sharer.php?u=${u}`,
    },
    { key: "x", label: "X", href: `https://x.com/intent/post?text=${text}&url=${u}` },
    {
      key: "linkedin",
      label: "LinkedIn",
      href: `https://www.linkedin.com/sharing/share-offsite/?url=${u}`,
    },
    {
      key: "bluesky",
      label: "Bluesky",
      href: `https://bsky.app/intent/compose?text=${textAndUrl}`,
    },
    { key: "whatsapp", label: "WhatsApp", href: `https://wa.me/?text=${textAndUrl}` },
    { key: "telegram", label: "Telegram", href: `https://t.me/share/url?url=${u}&text=${text}` },
    {
      key: "email",
      label: "E-mail",
      href: `mailto:?subject=${text}&body=${encodeURIComponent(`${shareText(title)}\n\n${url}`)}`,
    },
  ];
}
