import { describe, expect, it } from "vitest";

import { isValidEmail, mailtoHref } from "./email";

// Finding INJ-7: an address carrying RFC 6068 header fields must never become a mailto link.
const hostile = [
  "contact@mairie.fr?cc=attacker%40evil.example&subject=Re%3A%20droit%20de%20r%C3%A9ponse",
  "a@b.c?body=Cliquez%20https%3A%2F%2Fevil.example",
  "a@b.fr&bcc=attacker@evil.example",
  "a%40evil.example@b.fr",
  "a@b.fr#fragment",
  "a#b@c.fr",
  "a@b.fr,attacker@evil.example",
  "a@b.fr;attacker@evil.example",
  "a b@c.fr",
  "a@b c.fr",
  "a@b.fr\ncc:attacker@evil.example",
  ".a@b.fr",
  "a.@b.fr",
  "a..b@c.fr",
  "a@-b.fr",
  "a@b-.fr",
  "a@b..fr",
  "a@b.f",
  "a@b.123",
  "a@localhost",
  "@b.fr",
  "a@",
  "a@@b.fr",
  `${"a".repeat(65)}@b.fr`,
  `a@${"b".repeat(250)}.fr`,
];

const valid = [
  "dominique@example.org",
  "jean.dupont+signalement@mairie-de-lyon.fr",
  "o'neil@example.co.uk",
  "contact@xn--crche-cva.fr",
  "contact@crèche.fr",
  "a@b.museum",
];

describe("e-mail addresses", () => {
  it.each(hostile)("rejects %j", (value) => {
    expect(isValidEmail(value)).toBe(false);
    expect(mailtoHref(value)).toBeNull();
  });

  it.each(valid)("accepts %j", (value) => {
    expect(isValidEmail(value)).toBe(true);
    expect(mailtoHref(value)).toBe(`mailto:${value}`);
  });

  it("builds no link without an address", () => {
    expect(mailtoHref(null)).toBeNull();
    expect(mailtoHref(undefined)).toBeNull();
    expect(mailtoHref("")).toBeNull();
  });
});
