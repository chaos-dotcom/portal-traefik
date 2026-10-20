import type { PageLoad } from "./$types";
import type { LinkItem } from "$lib/links";

export const load: PageLoad = async ({ fetch }) => {
  const res = await fetch("/api/links");
  const links: LinkItem[] = res.ok ? await res.json() : [];
  return { links, failed: !res.ok };
};
