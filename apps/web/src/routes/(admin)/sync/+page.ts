import type { PageLoad } from "./$types";
import type { SyncRule } from "../sync-rules/+page";

export type { SyncRule };

export const load: PageLoad = async ({ fetch }) => {
  const res = await fetch("/api/sync-rules");
  const rules: SyncRule[] = res.ok ? await res.json() : [];
  return { rules, failed: !res.ok };
};
