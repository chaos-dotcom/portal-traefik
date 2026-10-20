import { error } from "@sveltejs/kit";
import type { PageLoad } from "./$types";
import { loadComposerContext } from "$lib/composer";
import type { UploadLink, UploadSessionRow } from "$lib/links";

export const load: PageLoad = async ({ params, fetch }) => {
  const [linkRes, sessionsRes, ctx] = await Promise.all([
    fetch(`/api/upload-links/${params.id}`),
    fetch(`/api/upload-links/${params.id}/sessions?limit=25`),
    loadComposerContext(fetch),
  ]);
  if (!linkRes.ok) throw error(linkRes.status === 404 ? 404 : 500, "Upload link not found");
  const link: UploadLink = await linkRes.json();
  const sessions: UploadSessionRow[] = sessionsRes.ok ? await sessionsRes.json() : [];
  return { link, sessions, ...ctx };
};
