import { redirect } from "@sveltejs/kit";
import type { PageLoad } from "./$types";
import type { PathSegment } from "$lib/frameio";

export type SyncRuleSource = {
  type?: string;
  account_id?: string;
  workspace_id?: string;
  project_id?: string;
  folder_id?: string;
  folder_name?: string;
  project_name?: string;
  recursive?: boolean;
  // Cached Frame.io breadcrumb, root→leaf (may be absent on legacy rows).
  path?: PathSegment[];
};

// What a rule is doing (GET /api/sync-rules adds this to each rule).
export type SyncActivity = {
  counts: Partial<Record<"pending" | "running" | "waiting" | "done" | "skipped" | "dead_letter", number>>;
  bytes_done: number;
  last_done_at: string | null;
  last_done_file: string | null;
  running_file: string | null;
  waiting_file: string | null;
  failed_file: string | null;
};

export type SyncRule = {
  id: string;
  name: string;
  source: SyncRuleSource;
  destination_path: string;
  conflict_policy: string;
  path_template: string | null;
  enabled: boolean;
  subscription_active: boolean;
  created_at: string;
  activity?: SyncActivity;
};

export const load: PageLoad = async () => {
  throw redirect(302, "/sync");
};
