// Recent Frame.io folders: the last one used here first, then the folders existing upload links
// deliver into, newest first. This is what destinations used to be for: getting back to a place.
import { folderKey, type FolderRef } from "$lib/frameio";

export function lastFolder(): FolderRef | null {
  try {
    const raw = localStorage.getItem("portal.lastFolder");
    return raw ? (JSON.parse(raw) as FolderRef) : null;
  } catch {
    return null;
  }
}

export function rememberFolder(f: FolderRef) {
  try {
    localStorage.setItem("portal.lastFolder", JSON.stringify(f));
  } catch {
    /* private mode */
  }
}

export function recentFolders(links: { kind: string; folder?: FolderRef | null; created_at: string }[], limit = 8): FolderRef[] {
  const out: FolderRef[] = [];
  const seen = new Set<string>();
  const push = (f: FolderRef | null | undefined) => {
    if (!f?.account_id || !f.folder_id) return;
    const k = folderKey(f);
    if (seen.has(k)) return;
    seen.add(k);
    out.push(f);
  };
  push(lastFolder());
  [...links]
    .filter((l) => l.kind === "upload")
    .sort((a, b) => b.created_at.localeCompare(a.created_at))
    .forEach((l) => push(l.folder));
  return out.slice(0, limit);
}
