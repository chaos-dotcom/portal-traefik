// What every link composer page loads besides the link itself: recent folders (from existing
// upload links), sync rules (to say whether a folder lands on the NAS), the page branding for the
// preview, and whether email is set up (for "Email to client").
import type { Branding } from "$lib/components";
import type { FolderRef, RuleLike } from "$lib/frameio";
import type { UploadLink } from "$lib/links";
import { recentFolders } from "$lib/recents";

type Fetch = typeof fetch;

export async function loadComposerContext(fetch: Fetch) {
  const [upRes, rulesRes, brandRes, emailRes] = await Promise.all([
    fetch("/api/upload-links"),
    fetch("/api/sync-rules"),
    fetch("/api/settings/branding"),
    fetch("/api/settings/email"),
  ]);
  const uploads: UploadLink[] = upRes.ok ? await upRes.json() : [];
  const rules: RuleLike[] = rulesRes.ok ? await rulesRes.json() : [];
  const b = brandRes.ok ? await brandRes.json() : {};
  const email = emailRes.ok ? await emailRes.json() : {};
  const branding: Branding = {
    name: b.brand_display_name ?? null,
    accent: b.brand_accent_color ?? null,
    logo_url: b.has_logo ? `/api/branding/logo?v=${b.logo_updated_at ? Date.parse(b.logo_updated_at) : 0}` : null,
  };
  return {
    uploads,
    rules,
    branding,
    emailConfigured: email.configured !== false,
    // Recents read localStorage, so they're computed in the browser by the page.
    recentsFrom: uploads as { kind: string; folder?: FolderRef | null; created_at: string }[],
  };
}

export { recentFolders };
