import { paths } from "@/app/paths";

export interface NavItem {
  to: string;
  label: string;
  /** Only active on an exact match (needed for "/"). */
  end?: boolean;
}

export const mainNavItems: readonly NavItem[] = [
  { to: paths.home, label: "Classement", end: true },
  { to: paths.propose, label: "Proposer une idée" },
  { to: paths.methodology, label: "Méthodologie" },
  { to: paths.about, label: "À propos" },
];

export const legalNavItems: readonly NavItem[] = [
  { to: paths.legalNotice, label: "Mentions légales" },
  { to: paths.privacy, label: "Confidentialité" },
  { to: paths.terms, label: "Règles du site (CGU)" },
  { to: paths.licence, label: "Licence et réutilisation" },
];
