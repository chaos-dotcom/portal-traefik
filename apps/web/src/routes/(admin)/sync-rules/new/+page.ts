import type { PageLoad } from "./$types";
import type { FolderRef } from "$lib/frameio";
import type { SyncRule } from "../+page";
import type { UploadLink } from "$lib/links";

export const load: PageLoad = async ({ fetch, url }) => {
  const [rulesRes, upRes] = await Promise.all([fetch("/api/sync-rules"), fetch("/api/upload-links")]);
  const rules: SyncRule[] = rulesRes.ok ? await rulesRes.json() : [];
  const uploads: UploadLink[] = upRes.ok ? await upRes.json() : [];
  // "Sync this folder" from a link: ?account_id=&folder_id=
  let folder: FolderRef | null = null;
  const account = url.searchParams.get("account_id");
  const folderId = url.searchParams.get("folder_id");
  if (account && folderId) {
    const res = await fetch(`/api/frameio/path?account_id=${encodeURIComponent(account)}&folder_id=${encodeURIComponent(folderId)}`);
    if (res.ok) folder = { type: "frameio", ...(await res.json()) };
  }
  return { rules, uploads, folder };
};
