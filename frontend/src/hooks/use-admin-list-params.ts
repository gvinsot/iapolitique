import { useSearchParams } from "react-router";

/** Filters and page of an admin list, kept in the URL so a view can be reloaded or shared. */
export function useAdminListParams<K extends string>(keys: readonly K[]) {
  const [params, setParams] = useSearchParams();
  const values = Object.fromEntries(keys.map((key) => [key, params.get(key) ?? ""])) as Record<
    K,
    string
  >;
  const rawPage = Number(params.get("page"));
  const page = Number.isInteger(rawPage) && rawPage >= 1 && rawPage <= 10_000 ? rawPage : 1;

  const set = (key: K, value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    next.delete("page");
    setParams(next, { replace: true });
  };

  const hrefForPage = (target: number) => {
    const next = new URLSearchParams(params);
    if (target > 1) next.set("page", String(target));
    else next.delete("page");
    const query = next.toString();
    return query ? `?${query}` : "?";
  };

  return { values, page, set, hrefForPage };
}
