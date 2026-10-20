import type { PageLoad } from "./$types";
import type { LinkItem } from "$lib/links";

export type ActiveUpload = {
  id: string;
  who: string | null;
  brand: string | null;
  uploaded_bytes: number | null;
  total_bytes: number | null;
  started_at: string | null;
  link_id?: string;
  last_activity_at?: string | null;
};

export type ActiveSync = {
  id: string;
  status: "running" | "pending";
  file: string | null;
  rule: string;
  rule_id: string;
  started_at: string | null;
};

export type SyncHealth = {
  waiting: number;
  running: number;
  queued: number;
  dead_letter: number;
  done_24h: number;
  dead_letter_rule_id?: string | null;
};

export type Transfers = {
  active_uploads: ActiveUpload[];
  active_syncs: ActiveSync[];
  sync_health: SyncHealth;
};

export type HistoryItem = {
  id: string;
  kind: "upload" | "download" | "sync";
  label: string | null;
  who: string | null;
  bytes: number | null;
  files?: number;
  status: string;
  at: string;
  error?: string | null;
  link_id?: string;
  rule_id?: string;
  via?: string | null;
};

const EMPTY: Transfers = {
  active_uploads: [],
  active_syncs: [],
  sync_health: { waiting: 0, running: 0, queued: 0, dead_letter: 0, done_24h: 0, dead_letter_rule_id: null },
};

export const load: PageLoad = async ({ fetch }) => {
  const [transfersRes, linksRes, connRes] = await Promise.all([
    fetch("/api/dashboard/transfers?limit=1"),
    fetch("/api/links"),
    fetch("/api/frameio/status"),
  ]);
  const transfers: Transfers = transfersRes.ok ? { ...EMPTY, ...(await transfersRes.json()) } : EMPTY;
  const links: LinkItem[] = linksRes.ok ? await linksRes.json() : [];
  const connected: boolean = connRes.ok ? (await connRes.json()).connected : false;
  return { transfers, links, connected };
};
