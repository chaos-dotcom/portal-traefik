import { error } from "@sveltejs/kit";
import type { PageLoad } from "./$types";
import { loadComposerContext } from "$lib/composer";
import type { DownloadEvent, DownloadLink } from "$lib/links";

export type DownloadStats = { downloads: number; unique_viewers: number; last_activity: string | null };

export const load: PageLoad = async ({ params, fetch }) => {
  const [linkRes, eventsRes, statsRes, ctx] = await Promise.all([
    fetch(`/api/download-links/${params.id}`),
    fetch(`/api/download-links/${params.id}/events`),
    fetch(`/api/download-links/${params.id}/stats`),
    loadComposerContext(fetch),
  ]);
  if (!linkRes.ok) throw error(linkRes.status === 404 ? 404 : 500, "Download link not found");
  const link: DownloadLink = await linkRes.json();
  const events: DownloadEvent[] = eventsRes.ok ? await eventsRes.json() : [];
  const stats: DownloadStats = statsRes.ok ? await statsRes.json() : { downloads: 0, unique_viewers: 0, last_activity: null };
  return { link, events, stats, ...ctx };
};
