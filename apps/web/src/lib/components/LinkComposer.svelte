<script lang="ts" module>
  export type Branding = { name: string | null; accent: string | null; logo_url: string | null };

  // File-type groups for upload links. "Anything" stores no restriction at all.
  export const TYPE_GROUPS = {
    video: { label: "Video", ext: ["r3d", "braw", "ari", "arx", "mxf", "mov", "mp4", "mts", "m4v", "avi", "prores"] },
    audio: { label: "Audio", ext: ["wav", "aif", "aiff", "mp3", "bwf"] },
    stills: { label: "Stills", ext: ["dng", "cdng", "exr", "dpx", "tiff", "tif", "png", "jpg", "jpeg"] },
    docs: { label: "Documents", ext: ["pdf", "edl", "xml", "ale", "aaf", "fcpxml", "otio", "txt", "csv", "zip"] },
  } as const;
  export type TypeGroup = keyof typeof TYPE_GROUPS;
</script>

<script lang="ts">
  // Create or edit a link — both kinds — on one screen. The minimum is three decisions: receive or
  // send, which folder (or files), and how long. Everything else has a default and waits under
  // More options. The right column shows the client's page as you build it.
  import { goto, invalidateAll } from "$app/navigation";
  import { onMount, untrack } from "svelte";
  import FolderField from "./FolderField.svelte";
  import FrameioPicker, { type SourcePick } from "./FrameioPicker.svelte";
  import Icon from "./Icon.svelte";
  import Switch from "./Switch.svelte";
  import ConfirmButton from "./ConfirmButton.svelte";
  import { coveringRules, crumbNames, nasPathFor, type DownloadSource, type FolderRef, type RuleLike } from "$lib/frameio";
  import { expiresText, kindNoun, sourceSummary, type DownloadLink, type UploadLink } from "$lib/links";
  import { copyWithToast, errorDetail, toast } from "$lib/toast.svelte";
  import { formatBytes, accentText } from "$lib/format";
  import { UPLOAD_TOKENS } from "$lib/tokens";
  import { rememberFolder } from "$lib/recents";

  type Kind = "upload" | "download";
  type Expiry = "keep" | "1" | "7" | "30" | "never" | "custom";

  let {
    mode = "create",
    kind: initialKind = "upload",
    link = null,
    recents = [],
    rules = [],
    branding,
    initialFolder = null,
    openSourcePicker = false,
    emailConfigured = true,
  }: {
    mode?: "create" | "edit";
    kind?: Kind;
    link?: UploadLink | DownloadLink | null;
    recents?: FolderRef[];
    rules?: RuleLike[];
    branding: Branding;
    initialFolder?: FolderRef | null;
    openSourcePicker?: boolean;
    emailConfigured?: boolean;
  } = $props();

  const editing = $derived(mode === "edit");
  const up = $derived(link?.kind === "upload" ? (link as UploadLink) : null);
  const down = $derived(link?.kind === "download" ? (link as DownloadLink) : null);

  // ── form state ──────────────────────────────────────────────────────────
  let kind = $state<Kind>(untrack(() => link?.kind ?? initialKind));
  let folder = $state<FolderRef | null>(null);
  let source = $state<SourcePick | null>(null);
  let name = $state("");
  let nameTouched = $state(false);
  let title = $state("");
  let subtitle = $state("");
  let expiry = $state<Expiry>("7");
  let customDate = $state("");
  let pwMode = $state<"none" | "keep" | "set" | "clear">("none");
  let password = $state("");
  let otp = $state(false);
  let askName = $state(false);
  let askEmail = $state(false);
  let askMessage = $state(false);
  let types = $state<Set<TypeGroup | "any">>(new Set(["any"]));
  let customExt = $state<string[]>([]);
  let maxSizeGb = $state("");
  let template = $state("");
  let maxDownloads = $state("");
  let preview = $state(true);
  let preset = $state("");
  let moreOpen = $state(false);
  let sourceOpen = $state(false);

  let saving = $state(false);
  let error = $state<string | null>(null);
  let created = $state<(UploadLink | DownloadLink) | null>(null);
  let copied = $state(false);
  let emailOpen = $state(false);
  let emailTo = $state("");
  let emailNote = $state("");
  let emailSending = $state(false);

  // Seed from the link being edited (or sensible defaults when creating).
  function seed() {
    error = null;
    if (link) {
      name = link.name;
      nameTouched = true;
      title = link.brand_display_name ?? "";
      subtitle = link.brand_subtitle ?? "";
      expiry = link.expires_at ? "keep" : "never";
      pwMode = link.password_protected ? "keep" : "none";
      password = "";
      otp = link.verify_email;
      if (link.kind === "upload") {
        folder = link.folder;
        askName = link.uploader_fields_required.name;
        askEmail = link.uploader_fields_required.email && !link.verify_email;
        askMessage = link.uploader_fields_required.message;
        const ext = link.allowed_extensions;
        if (!ext?.length) {
          types = new Set(["any"]);
          customExt = [];
        } else {
          const groups = (Object.keys(TYPE_GROUPS) as TypeGroup[]).filter((g) => (TYPE_GROUPS[g].ext as readonly string[]).some((e) => ext.includes(e)));
          const known = new Set<string>(groups.flatMap((g) => [...TYPE_GROUPS[g].ext]));
          types = new Set(groups.length ? groups : []);
          customExt = ext.filter((e) => !known.has(e));
          if (!groups.length && !customExt.length) types = new Set(["any"]);
        }
        maxSizeGb = link.max_file_size ? String(+(link.max_file_size / 1024 ** 3).toFixed(2)) : "";
        template = link.subfolder_template ?? "";
      } else {
        const s = link.source;
        source = { source: s as DownloadSource, label: (s.label as string) ?? sourceSummary(s), path: (s.path as FolderRef["path"]) ?? [], count: null, bytes: (s.bytes as number) ?? null };
        askName = link.viewer_fields_required.name;
        askEmail = link.viewer_fields_required.email && !link.verify_email;
        maxDownloads = link.max_downloads ? String(link.max_downloads) : "";
        preview = link.allow_preview;
      }
    } else {
      folder = initialFolder;
      expiry = "7";
      name = initialFolder ? (crumbNames(initialFolder).at(-1) ?? "") : "";
    }
  }
  seed();

  onMount(() => {
    if (openSourcePicker && kind === "download" && !source) sourceOpen = true;
  });

  // ── derived ─────────────────────────────────────────────────────────────
  const coverage = $derived(kind === "upload" ? coveringRules(folder, rules) : []);
  const enabledCoverage = $derived(coverage.filter((c) => c.enabled));
  const accent = $derived(branding.accent || "#f59e0b");
  const brandName = $derived(branding.name || "Portal");

  function endOfDay(days: number): Date {
    const d = new Date();
    d.setDate(d.getDate() + days);
    d.setHours(23, 59, 0, 0);
    return d;
  }
  const expiresAt = $derived.by((): Date | null | "keep" => {
    if (expiry === "keep") return "keep";
    if (expiry === "never") return null;
    if (expiry === "custom") return customDate ? new Date(`${customDate}T23:59:00`) : null;
    return endOfDay(Number(expiry));
  });
  const fmtDay = (d: Date) => d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
  const expiryHelp = $derived.by(() => {
    if (expiresAt === "keep") return link?.expires_at ? (link.state === "expired" ? `Expired ${fmtDay(new Date(link.expires_at))}. Pick a new length to reopen it.` : `Closes ${fmtDay(new Date(link.expires_at))}.`) : "";
    if (expiresAt === null) return expiry === "custom" ? "Pick the last day it should work." : "Stays open until you revoke it.";
    return `Closes ${fmtDay(expiresAt)} at 23:59.`;
  });
  const expiryShort = $derived.by(() => {
    if (expiresAt === "keep") return link ? `Open ${expiresText(link)}` : "";
    if (expiresAt === null) return expiry === "custom" ? "" : "No expiry";
    return `Open until ${fmtDay(expiresAt)}`;
  });

  const allowedExtensions = $derived.by((): string[] | null => {
    if (types.has("any")) return customExt.length ? null : null;
    const ext = [...types].flatMap((g) => [...TYPE_GROUPS[g as TypeGroup].ext]);
    const all = [...ext, ...customExt];
    return all.length ? all : null;
  });
  const typesLabel = $derived(
    types.has("any") || !allowedExtensions
      ? "Any file type"
      : [...[...types].map((g) => TYPE_GROUPS[g as TypeGroup].label), ...(customExt.length ? [customExt.map((e) => `.${e}`).join(" ")] : [])].join(", "),
  );

  const tplExample = $derived.by(() => {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const vals: Record<string, string> = {
      date: `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`,
      year: String(d.getFullYear()),
      month: pad(d.getMonth() + 1),
      day: pad(d.getDate()),
      time: `${pad(d.getHours())}h${pad(d.getMinutes())}`,
      uploader_name: "Ana Ribeiro",
    };
    const parts = template.trim() ? template.replace(/\{(\w+)\}/g, (m, k) => vals[k] ?? m).split("/").map((x) => x.trim()).filter(Boolean) : [];
    return [crumbNames(folder).at(-1) ?? "Folder", ...parts];
  });

  const moreSummary = $derived(
    kind === "upload"
      ? [typesLabel, maxSizeGb ? `up to ${maxSizeGb} GB per file` : "no size limit", template.trim() ? "sorted into folders" : ""].filter(Boolean).join(" · ")
      : [maxDownloads ? `${maxDownloads} downloads max` : "unlimited downloads", preview ? "thumbnails on" : "thumbnails off"].join(" · "),
  );

  // ── interactions ────────────────────────────────────────────────────────
  function setKind(k: Kind) {
    if (editing || k === kind) return;
    kind = k;
    if (!nameTouched) name = k === "upload" ? (crumbNames(folder).at(-1) ?? "") : (source?.label ?? "");
    if (k === "download" && !source) sourceOpen = true;
  }

  function onFolder(f: FolderRef) {
    if (!nameTouched) name = crumbNames(f).at(-1) ?? "";
    rememberFolder(f);
  }

  function onSource(p: SourcePick) {
    source = p;
    if (!nameTouched) name = p.label;
  }

  function toggleType(g: TypeGroup | "any") {
    const next = new Set(types);
    if (g === "any") {
      types = new Set(["any"]);
      customExt = [];
      return;
    }
    next.delete("any");
    if (next.has(g)) next.delete(g);
    else next.add(g);
    if (!next.size && !customExt.length) next.add("any");
    types = next;
  }

  function insertToken(tok: string) {
    const t = `{${tok}}`;
    if (template.includes(t)) {
      template = template.replace(`${t}/`, "").replace(`/${t}`, "").replace(t, "");
      return;
    }
    template = template && !template.endsWith("/") ? `${template}/${t}` : `${template}${t}`;
  }

  const WORDS = ["amber", "slate", "grain", "velvet", "reel", "frame", "focus", "prism", "dolly", "lens", "chroma", "tide", "atlas", "ember", "cedar", "orbit"];
  function passphrase(): string {
    const pick = () => WORDS[Math.floor(Math.random() * WORDS.length)];
    return `${pick()}-${pick()}-${10 + Math.floor(Math.random() * 90)}`;
  }

  function applyPreset(p: string) {
    if (p === "dailies") {
      if (!editing) kind = "upload";
      expiry = "7";
      otp = true;
      askName = true;
      types = new Set(["video", "audio"]);
      customExt = [];
      template = "{date}/{uploader_name}";
      toast("Dailies drop: 7 days, video and audio, email code, sorted by date and uploader");
    } else if (p === "finals") {
      if (!editing) kind = "download";
      expiry = "30";
      pwMode = pwMode === "keep" ? "keep" : "set";
      if (pwMode === "set" && !password) password = passphrase();
      otp = false;
      preview = true;
      toast("Final delivery: 30 days, password, thumbnails");
      if (kind === "download" && !source) sourceOpen = true;
    }
    preset = "";
  }

  // ── save ────────────────────────────────────────────────────────────────
  function body(): Record<string, unknown> {
    const b: Record<string, unknown> = {
      name: name.trim() || null,
      brand_display_name: title.trim() || null,
      brand_subtitle: subtitle.trim() || null,
      verify_email: otp,
    };
    if (expiresAt !== "keep") b.expires_at = expiresAt ? expiresAt.toISOString() : null;
    if (pwMode === "set" && password) b.password = password;
    if (pwMode === "clear") b.password = "";
    if (kind === "upload") {
      if (!editing || (folder && (folder.folder_id !== up?.folder?.folder_id || folder.account_id !== up?.folder?.account_id))) b.folder = folder;
      b.uploader_fields_required = { name: askName, email: askEmail || otp, message: askMessage };
      b.allowed_extensions = allowedExtensions;
      b.max_file_size = maxSizeGb ? Math.round(parseFloat(maxSizeGb) * 1024 ** 3) : null;
      b.subfolder_template = template.trim() || null;
    } else {
      if (!editing || JSON.stringify(source?.source) !== JSON.stringify(down?.source)) b.source = source?.source;
      b.viewer_fields_required = { name: askName, email: askEmail || otp };
      b.max_downloads = maxDownloads ? parseInt(maxDownloads, 10) : null;
      b.allow_preview = preview;
    }
    return b;
  }

  const ready = $derived(kind === "upload" ? !!folder : !!source);

  async function save() {
    error = null;
    if (!ready) {
      error = kind === "upload" ? "Choose the Frame.io folder uploads should land in." : "Choose the files or folder to send.";
      return;
    }
    if (expiry === "custom" && !customDate) {
      error = "Pick the date the link should stop working.";
      return;
    }
    if (pwMode === "set" && !password) {
      error = "Enter a password, or turn the password off.";
      return;
    }
    saving = true;
    try {
      const base = kind === "upload" ? "/api/upload-links" : "/api/download-links";
      const res = await fetch(editing ? `${base}/${link!.id}` : base, {
        method: editing ? "PATCH" : "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body()),
      });
      if (!res.ok) {
        error = await errorDetail(res, editing ? "Could not save" : "Could not create the link");
        return;
      }
      const out = await res.json();
      if (editing) {
        toast("Saved");
        await invalidateAll();
        seed();
      } else {
        created = out;
        copied = await copyWithToast(out.public_url, "Link created and copied");
      }
    } finally {
      saving = false;
    }
  }

  async function sendEmail() {
    const l = created ?? link;
    if (!l || !emailTo.trim()) return;
    emailSending = true;
    try {
      const res = await fetch("/api/links/email", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ kind: l.kind, id: l.id, to: emailTo.trim(), note: emailNote.trim() || null }),
      });
      if (!res.ok) {
        toast(await errorDetail(res, "Could not send the email"), { kind: "error" });
        return;
      }
      toast(`Sent to ${emailTo.trim()}`);
      emailOpen = false;
      emailTo = "";
      emailNote = "";
    } finally {
      emailSending = false;
    }
  }

  function createAnother() {
    created = null;
    emailOpen = false;
    nameTouched = false;
    title = "";
    password = "";
    pwMode = "none";
    name = kind === "upload" ? (crumbNames(folder).at(-1) ?? "") : "";
    if (kind === "download") source = null;
  }

  async function revoke() {
    if (!link) return;
    const base = link.kind === "upload" ? "/api/upload-links" : "/api/download-links";
    const res = await fetch(`${base}/${link.id}/revoke`, { method: "POST" });
    if (!res.ok) return toast(await errorDetail(res, "Could not revoke"), { kind: "error" });
    await invalidateAll();
    seed();
    toast(`“${link.name}” revoked`, {
      action: {
        label: "Undo",
        run: async () => {
          await fetch(`${base}/${link!.id}/restore`, { method: "POST" });
          await invalidateAll();
          seed();
        },
      },
    });
  }

  async function restore() {
    if (!link) return;
    const base = link.kind === "upload" ? "/api/upload-links" : "/api/download-links";
    await fetch(`${base}/${link.id}/restore`, { method: "POST" });
    await invalidateAll();
    seed();
    toast("Link restored");
  }

  async function remove() {
    if (!link) return;
    const base = link.kind === "upload" ? "/api/upload-links" : "/api/download-links";
    const res = await fetch(`${base}/${link.id}`, { method: "DELETE" });
    if (!res.ok) return toast(await errorDetail(res, "Could not delete"), { kind: "error" });
    toast(`Deleted “${link.name}”`);
    await goto("/links");
  }

  const chip = (on: boolean) =>
    `inline-flex h-7 items-center gap-1.5 rounded-full border px-3 text-[12.5px] font-medium transition-colors ${on ? "border-accent/50 bg-accent-soft text-accent-hover" : "border-border text-muted hover:border-border-strong hover:text-text"}`;
  const input = "h-8 w-full rounded-md border border-border bg-bg px-2.5 text-[13px] text-text outline-none placeholder:text-faint hover:border-border-strong focus:border-accent focus:ring-3 focus:ring-accent-soft";
  const label = "pt-2 text-xs font-medium text-faint";
  const line = "grid grid-cols-1 gap-2 border-t border-border py-3.5 sm:grid-cols-[104px_minmax(0,1fr)] sm:gap-4";

  const previewFiles = $derived(
    ((source?.source as { names?: string[] } | undefined)?.names ?? []).slice(0, 3),
  );
  const shownLink = $derived(created ?? link);
</script>

<div class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_340px]">
  <!-- ── form ── -->
  <div class="min-w-0 rounded-xl border border-border bg-surface text-[13px]">
    {#if created}
      <div class="grid justify-items-start gap-4 px-5 py-10 sm:px-7">
        <span class="grid h-10 w-10 place-items-center rounded-lg bg-success/15 text-success"><Icon name="check" size={20} /></span>
        <h2 class="text-lg font-semibold text-text">{copied ? "Link ready and copied" : "Link ready"}</h2>
        <div class="flex w-full max-w-xl items-center gap-2 rounded-lg border border-accent/40 bg-accent-soft py-1.5 pr-1.5 pl-3">
          <button type="button" class="min-w-0 flex-1 truncate text-left font-mono text-[13px] text-text hover:text-accent-hover" title="Copy link" onclick={() => copyWithToast(created!.public_url)}>{created.public_url.replace(/^https?:\/\//, "")}</button>
          <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-3 font-medium text-text hover:bg-surface-3" onclick={() => copyWithToast(created!.public_url)}>
            <Icon name="copy" size={15} />Copy
          </button>
        </div>
        <p class="max-w-xl text-muted">
          {#if created.kind === "upload"}
            Receives into {crumbNames((created as UploadLink).folder).join(" / ")}.
          {:else}
            Sends {source?.label ?? "the files you picked"}.
          {/if}
          {expiryShort}{otp ? ", email code on" : ""}{pwMode === "set" ? ", password protected" : ""}.
        </p>
        <div class="flex flex-wrap gap-2">
          <a href={created.public_url} target="_blank" rel="noopener" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-3 font-medium text-text hover:bg-surface-3">
            <Icon name="open" size={15} />Open client page
          </a>
          <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-3 font-medium text-text hover:bg-surface-3 disabled:opacity-40" disabled={!emailConfigured} title={emailConfigured ? undefined : "Set up email in Settings first"} onclick={() => (emailOpen = !emailOpen)}>
            <Icon name="mail" size={15} />Email to client…
          </button>
          <button type="button" class="inline-flex h-8 items-center rounded-md px-3 font-medium text-muted hover:bg-surface-3 hover:text-text" onclick={createAnother}>Create another</button>
          <a href="/links" class="inline-flex h-8 items-center rounded-md px-3 font-medium text-muted hover:bg-surface-3 hover:text-text">Back to links</a>
        </div>
      </div>
    {:else}
      <div class="flex items-center gap-3 border-b border-border px-5 py-3.5 sm:px-7">
        <div class="min-w-0 flex-1">
          <h1 class="truncate text-base font-semibold text-text">{editing ? link?.name : "New link"}</h1>
          {#if editing && link}
            <div class="mt-0.5 flex min-w-0 items-center gap-2 text-xs text-faint">
              <button type="button" class="truncate font-mono hover:text-accent-hover" title="Copy link" onclick={() => copyWithToast(link!.public_url)}>{link.public_url.replace(/^https?:\/\//, "")}</button>
            </div>
          {/if}
        </div>
        {#if editing && link}
          <button type="button" class="grid h-8 w-8 place-items-center rounded-md text-muted hover:bg-surface-3 hover:text-text" title="Copy link" aria-label="Copy link" onclick={() => copyWithToast(link!.public_url)}><Icon name="copy" /></button>
          <a href={link.public_url} target="_blank" rel="noopener" class="grid h-8 w-8 place-items-center rounded-md text-muted hover:bg-surface-3 hover:text-text" title="Open client page" aria-label="Open client page"><Icon name="open" /></a>
        {/if}
        <a href="/links" class="grid h-8 w-8 place-items-center rounded-md text-faint hover:bg-surface-3 hover:text-text" aria-label="Close"><Icon name="x" /></a>
      </div>

      <div class="px-5 pt-4 pb-2 sm:px-7">
        {#if error}<p class="mb-3 rounded-md bg-danger/10 px-3 py-2 text-danger">{error}</p>{/if}
        {#if editing && link?.state === "revoked"}
          <p class="mb-3 flex items-center gap-3 rounded-md bg-surface-2 px-3 py-2 text-muted">
            <span class="flex-1">This link is revoked. Clients see a closed page.</span>
            <button type="button" class="font-medium text-accent-hover hover:underline" onclick={restore}>Restore</button>
          </p>
        {/if}

        {#if !editing}
          <div class="mb-1 grid grid-cols-1 gap-2.5 sm:grid-cols-2" role="radiogroup" aria-label="Link type">
            {#each [{ k: "upload", icon: "receive", t: "Receive files", s: "Clients upload into Frame.io" }, { k: "download", icon: "send", t: "Send files", s: "Clients download from Frame.io" }] as o (o.k)}
              <button
                type="button"
                role="radio"
                aria-checked={kind === o.k}
                onclick={() => setKind(o.k as Kind)}
                class="flex items-center gap-3 rounded-lg border px-3.5 py-3 text-left transition-colors {kind === o.k ? 'border-accent/50 bg-accent-soft' : 'border-border bg-surface-2/40 hover:border-border-strong'}"
              >
                <span class="grid h-9 w-9 shrink-0 place-items-center rounded-lg {kind === o.k ? 'bg-accent text-on-accent' : 'bg-surface-3 text-body'}"><Icon name={o.icon as "receive" | "send"} size={18} /></span>
                <span><b class="block text-[13.5px] font-semibold text-text">{o.t}</b><small class="block text-xs text-faint">{o.s}</small></span>
              </button>
            {/each}
          </div>
        {/if}

        {#if kind === "upload"}
          <div class="{line} {editing ? 'border-t-0' : ''}">
            <div class={label}>Into</div>
            <div class="grid min-w-0 gap-2">
              <FolderField bind:value={folder} {recents} title="Receive into" chooseVerb="Use" onchange={onFolder} />
              {#if folder}
                <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-muted">
                  {#if enabledCoverage.length}
                    <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-success"></span>
                    Lands on the NAS at <span class="font-mono text-[11.5px] break-all text-text">{nasPathFor(folder, enabledCoverage[0], rules)}</span>
                    <span class="text-faint">via “{enabledCoverage[0].name}”</span>
                  {:else}
                    <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-faint"></span>
                    Stays in Frame.io only.
                    <a href="/sync-rules/new?account_id={encodeURIComponent(folder.account_id)}&folder_id={encodeURIComponent(folder.folder_id)}" class="text-accent-hover hover:underline">Sync this folder</a>
                  {/if}
                </div>
              {/if}
              {#if recents.length}
                <div class="flex flex-wrap items-center gap-1.5 text-xs text-faint">
                  <span class="mr-0.5">Recent</span>
                  {#each recents.filter((r) => r.folder_id !== folder?.folder_id).slice(0, 4) as r (r.account_id + r.folder_id)}
                    <button type="button" class="{chip(false)} h-6 px-2.5 text-xs" onclick={() => { folder = r; onFolder(r); }}>{crumbNames(r).slice(-2).join(" / ")}</button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {:else}
          <div class="{line} {editing ? 'border-t-0' : ''}">
            <div class={label}>Files</div>
            <div class="grid min-w-0 gap-2">
              <button type="button" onclick={() => (sourceOpen = true)} class="flex h-8 w-full min-w-0 items-center gap-1.5 rounded-md border border-border bg-bg pr-2 pl-2 text-left hover:border-border-strong">
                <span class="text-faint"><Icon name={source?.source.type === "folder" ? "folder" : "file"} /></span>
                {#if source}
                  {@const where = crumbNames({ account_id: "", folder_id: "", path: source.path })}
                  <span class="min-w-0 flex-1 truncate text-muted">{where.join(" / ")}{#if source.source.type !== "folder"}{where.length ? " / " : ""}<span class="text-text">{source.label}</span>{/if}</span>
                {:else}
                  <span class="flex-1 truncate text-faint">Choose files or a folder…</span>
                {/if}
                <span class="text-faint"><Icon name="chevron" size={14} /></span>
              </button>
              {#if source}
                <div class="flex items-center gap-2 text-[12.5px] text-muted">
                  <span class="h-1.5 w-1.5 rounded-full bg-success"></span>
                  {sourceSummary(source.source as DownloadLink["source"])}
                  {#if source.source.type === "folder"}<span class="text-faint">· clients get what's in it when they open the link</span>{/if}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        <div class={line}>
          <div class={label}><label for="lc-name">Name</label></div>
          <div class="grid gap-1.5">
            <input id="lc-name" class={input} bind:value={name} oninput={() => (nameTouched = true)} placeholder={kind === "upload" ? "e.g. Day 3 camera cards" : "e.g. Grade v4 selects"} autocomplete="off" />
            <div class="text-xs text-faint">Only you see this. It's how the link shows up in your list.</div>
          </div>
        </div>

        <div class={line}>
          <div class={label}><label for="lc-title">Page</label></div>
          <div class="grid gap-2 sm:grid-cols-2">
            <input id="lc-title" class={input} bind:value={title} placeholder={kind === "upload" ? "Send us your files" : "Your files are ready"} aria-label="Page title" autocomplete="off" />
            <input class={input} bind:value={subtitle} placeholder="Subtitle (optional)" aria-label="Page subtitle" autocomplete="off" />
          </div>
        </div>

        <div class={line}>
          <div class={label}>Expires</div>
          <div class="grid gap-2">
            <div class="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Expiry">
              {#if editing && link?.expires_at}
                <button type="button" class={chip(expiry === "keep")} onclick={() => (expiry = "keep")}>Keep {expiresText(link)}</button>
              {/if}
              {#each [["1", "1 day"], ["7", "7 days"], ["30", "30 days"], ["never", "Never"], ["custom", "Pick a date…"]] as [v, l] (v)}
                <button type="button" class={chip(expiry === v)} onclick={() => (expiry = v as Expiry)}>{l}</button>
              {/each}
            </div>
            {#if expiry === "custom"}
              <input type="date" class="{input} max-w-48" bind:value={customDate} aria-label="Last day" />
            {/if}
            <div class="text-xs text-faint">{expiryHelp}</div>
          </div>
        </div>

        <div class={line}>
          <div class={label}>Protect</div>
          <div class="grid gap-3">
            <div class="flex items-center gap-3">
              <div class="min-w-0 flex-1">
                <b class="block font-medium text-text">Email code</b>
                <small class="block text-xs text-faint">{kind === "upload" ? "Uploaders" : "Recipients"} confirm their email with a 6-digit code first</small>
              </div>
              <Switch bind:checked={otp} label="Email code" />
            </div>
            <div class="flex items-center gap-3">
              <div class="min-w-0 flex-1">
                <b class="block font-medium text-text">Password</b>
                <small class="block text-xs text-faint">
                  {pwMode === "keep" ? "A password is set." : pwMode === "clear" ? "The password will be removed when you save." : "Share it separately from the link."}
                </small>
              </div>
              {#if pwMode === "keep"}
                <button type="button" class="text-xs font-medium text-muted hover:text-text" onclick={() => { pwMode = "set"; password = passphrase(); }}>Change</button>
                <button type="button" class="text-xs font-medium text-muted hover:text-danger" onclick={() => (pwMode = "clear")}>Remove</button>
              {:else if pwMode === "clear"}
                <button type="button" class="text-xs font-medium text-muted hover:text-text" onclick={() => (pwMode = "keep")}>Keep it</button>
              {:else}
                <Switch
                  checked={pwMode === "set"}
                  label="Password"
                  onchange={(v) => {
                    pwMode = v ? "set" : link?.password_protected ? "clear" : "none";
                    if (v && !password) password = passphrase();
                  }}
                />
              {/if}
            </div>
            {#if pwMode === "set"}
              <input class="{input} max-w-72 font-mono" bind:value={password} aria-label="Password" autocomplete="off" spellcheck="false" />
            {/if}
            <div class="flex flex-wrap items-center gap-1.5">
              <span class="mr-1 text-xs text-faint">Ask for</span>
              <button type="button" class="{chip(askName)} h-6 px-2.5 text-xs" onclick={() => (askName = !askName)}>Name</button>
              <button type="button" class="{chip(askEmail || otp)} h-6 px-2.5 text-xs disabled:cursor-default" disabled={otp} title={otp ? "Needed for the email code" : undefined} onclick={() => (askEmail = !askEmail)}>Email</button>
              {#if kind === "upload"}
                <button type="button" class="{chip(askMessage)} h-6 px-2.5 text-xs" onclick={() => (askMessage = !askMessage)}>Message</button>
              {/if}
            </div>
          </div>
        </div>

        <details class="border-t border-border" bind:open={moreOpen}>
          <summary class="flex cursor-pointer list-none items-center gap-1.5 py-3 font-medium text-muted hover:text-text [&::-webkit-details-marker]:hidden">
            <span class="inline-block w-3 transition-transform {moreOpen ? 'rotate-90' : ''}">›</span>
            More options <span class="truncate font-normal text-faint">· {moreSummary}</span>
          </summary>
          {#if kind === "upload"}
            <div class="{line} border-t-0 pt-1">
              <div class={label}>File types</div>
              <div class="grid gap-2">
                <div class="flex flex-wrap gap-1.5">
                  {#each Object.entries(TYPE_GROUPS) as [g, def] (g)}
                    <button type="button" class={chip(types.has(g as TypeGroup))} onclick={() => toggleType(g as TypeGroup)}>{def.label}</button>
                  {/each}
                  <button type="button" class={chip(types.has("any"))} onclick={() => toggleType("any")}>Anything</button>
                </div>
                <div class="text-xs text-faint">
                  {#if allowedExtensions}Accepts {allowedExtensions.map((e) => `.${e}`).join(" ")}{:else}Every file type is accepted.{/if}
                </div>
              </div>
            </div>
            <div class={line}>
              <div class={label}><label for="lc-size">Max file size</label></div>
              <div class="relative max-w-44">
                <input id="lc-size" type="number" min="0" step="0.1" class="{input} pr-10" bind:value={maxSizeGb} placeholder="No limit" />
                <span class="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-xs text-faint">GB</span>
              </div>
            </div>
            <div class={line}>
              <div class={label}><label for="lc-tpl">Organize</label></div>
              <div class="grid min-w-0 gap-2">
                <input id="lc-tpl" class="{input} font-mono text-xs" bind:value={template} placeholder={"e.g. {date}/{uploader_name}"} autocomplete="off" spellcheck="false" />
                <div class="flex flex-wrap gap-1.5">
                  {#each UPLOAD_TOKENS as t (t.name)}
                    <button type="button" title={t.desc} class="{chip(template.includes(`{${t.name}}`))} h-6 px-2 font-mono text-[11.5px]" onclick={() => insertToken(t.name)}>{`{${t.name}}`}</button>
                  {/each}
                </div>
                <div class="rounded-md border border-border bg-surface-2/60 px-2.5 py-1.5 font-mono text-[11.5px] break-all text-muted">
                  {tplExample.join(" / ")} / <span class="text-text">A001_C003_1019XY.mov</span>
                </div>
              </div>
            </div>
          {:else}
            <div class="{line} border-t-0 pt-1">
              <div class={label}><label for="lc-maxdl">Limit</label></div>
              <div class="relative max-w-52">
                <input id="lc-maxdl" type="number" min="1" class="{input} pr-24" bind:value={maxDownloads} placeholder="Unlimited" />
                <span class="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-xs text-faint">downloads</span>
              </div>
            </div>
            <div class={line}>
              <div class={label}>Previews</div>
              <div class="flex items-center gap-3">
                <div class="min-w-0 flex-1">
                  <b class="block font-medium text-text">Show thumbnails</b>
                  <small class="block text-xs text-faint">Turn off for unreleased material</small>
                </div>
                <Switch bind:checked={preview} label="Show thumbnails" />
              </div>
            </div>
          {/if}
        </details>
      </div>

      <div class="flex flex-wrap items-center gap-2 border-t border-border px-5 py-3.5 sm:px-7">
        <label for="lc-preset" class="text-xs text-faint">Preset</label>
        <select id="lc-preset" class="h-8 rounded-md border border-border bg-bg px-2 text-[13px]" bind:value={preset} onchange={() => applyPreset(preset)}>
          <option value="">Apply…</option>
          <option value="dailies">Dailies drop</option>
          <option value="finals">Final delivery</option>
        </select>
        <span class="flex-1"></span>
        {#if editing && link}
          {#if link.state !== "revoked"}
            <ConfirmButton label="Revoke" confirmLabel="Revoke link" onconfirm={revoke} />
          {/if}
          <ConfirmButton label="Delete" confirmLabel="Delete forever" onconfirm={remove} />
        {:else}
          <a href="/links" class="inline-flex h-8 items-center rounded-md px-3 font-medium text-muted hover:bg-surface-3 hover:text-text">Cancel</a>
        {/if}
        <button type="button" class="inline-flex h-8 items-center rounded-md bg-accent px-4 font-medium text-on-accent hover:bg-accent-hover disabled:opacity-50" disabled={saving} onclick={save}>
          {saving ? (editing ? "Saving…" : "Creating…") : editing ? "Save changes" : "Create link"}
        </button>
      </div>
    {/if}

    {#if emailOpen && shownLink}
      <div class="grid gap-2.5 border-t border-border px-5 py-4 sm:px-7">
        <div class="text-xs font-medium text-faint">Email this link</div>
        <input class={input} type="email" bind:value={emailTo} placeholder="client@example.com" aria-label="Recipient" />
        <textarea class="{input} h-20 py-2" bind:value={emailNote} placeholder="A short note (optional)" aria-label="Note"></textarea>
        <div class="flex gap-2">
          <button type="button" class="inline-flex h-8 items-center rounded-md bg-accent px-4 font-medium text-on-accent hover:bg-accent-hover disabled:opacity-50" disabled={!emailTo.trim() || emailSending} onclick={sendEmail}>{emailSending ? "Sending…" : "Send"}</button>
          <button type="button" class="inline-flex h-8 items-center rounded-md px-3 font-medium text-muted hover:bg-surface-3 hover:text-text" onclick={() => (emailOpen = false)}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>

  <!-- ── preview ── -->
  <aside class="min-w-0 lg:sticky lg:top-24 lg:self-start" aria-label="Client page preview">
    <div class="mb-2 flex items-center justify-between">
      <span class="font-mono text-[10.5px] tracking-[0.08em] text-faint uppercase">Client sees</span>
      {#if editing && link && !created}
        <button type="button" class="text-xs text-muted hover:text-text disabled:opacity-40" disabled={!emailConfigured || link.state !== "ok"} title={emailConfigured ? undefined : "Set up email in Settings first"} onclick={() => (emailOpen = !emailOpen)}>Email to client…</button>
      {/if}
    </div>
    <div class="overflow-hidden rounded-xl border border-border bg-bg">
      <div class="flex h-8 items-center gap-2 border-b border-border bg-surface px-3">
        <span class="flex gap-1"><i class="h-2 w-2 rounded-full bg-surface-3"></i><i class="h-2 w-2 rounded-full bg-surface-3"></i><i class="h-2 w-2 rounded-full bg-surface-3"></i></span>
        {#if shownLink}
          <button type="button" class="truncate font-mono text-[10.5px] text-faint hover:text-accent-hover" title="Copy link" onclick={() => copyWithToast(shownLink!.public_url)}>{shownLink.public_url.replace(/^https?:\/\//, "")}</button>
        {:else}
          <span class="truncate font-mono text-[10.5px] text-faint">…/{kind === "upload" ? "u" : "d"}/new-link</span>
        {/if}
      </div>
      <div class="grid gap-2.5 px-6 pt-6 pb-5 text-center text-[12.5px]">
        {#if branding.logo_url}
          <img src={branding.logo_url} alt="" class="mx-auto mb-1 h-8 object-contain" />
        {:else}
          <div class="mb-1 text-[13px] font-bold tracking-[0.16em] text-text uppercase">{brandName}</div>
        {/if}
        <div class="text-base font-semibold break-words text-text">{title.trim() || (kind === "upload" ? "Send us your files" : "Your files are ready")}</div>
        {#if subtitle.trim()}<div class="-mt-1.5 text-muted">{subtitle}</div>{/if}
        {#if pwMode === "set" || pwMode === "keep"}<div class="flex h-8 items-center rounded-md border border-border px-2.5 text-left text-faint">Password</div>{/if}
        {#if askName}<div class="flex h-8 items-center rounded-md border border-border px-2.5 text-left text-faint">Your name</div>{/if}
        {#if askEmail || otp}<div class="flex h-8 items-center rounded-md border border-border px-2.5 text-left text-faint">Email</div>{/if}
        {#if kind === "upload" && askMessage}<div class="flex h-8 items-center rounded-md border border-border px-2.5 text-left text-faint">Message</div>{/if}
        {#if otp}<div class="-mt-1 text-[11.5px] text-faint">We'll email you a 6-digit code</div>{/if}
        {#if kind === "upload"}
          <div class="grid justify-items-center gap-1 rounded-lg border-[1.5px] border-dashed border-border-strong px-3 py-5 text-muted">
            <span style="color:{accent}"><Icon name="up" size={20} /></span>
            <b class="text-[13px] font-medium text-text">Drop files or folders</b>
            <span>{typesLabel}{maxSizeGb ? ` · up to ${maxSizeGb} GB each` : ""}</span>
          </div>
          <div class="grid h-9 place-items-center rounded-md text-[13px] font-semibold" style="background:{accent};color:{accentText(accent)}">Upload</div>
        {:else}
          <div class="grid gap-1.5 text-left">
            {#each previewFiles.length ? previewFiles : ["Your files appear here"] as f, i (i)}
              <div class="flex items-center gap-2 rounded-md border border-border p-1.5">
                {#if preview && previewFiles.length}
                  <span class="h-6 w-10 shrink-0 rounded bg-gradient-to-br from-[#3a2a1c] via-[#1e2f3a] to-[#0f1720]"></span>
                {:else}
                  <span class="grid h-6 w-6 shrink-0 place-items-center rounded bg-surface-2 text-faint"><Icon name="file" size={13} /></span>
                {/if}
                <span class="min-w-0 flex-1 truncate {previewFiles.length ? 'text-text' : 'text-faint'}">{f}</span>
              </div>
            {/each}
          </div>
          <div class="grid h-9 place-items-center rounded-md text-[13px] font-semibold" style="background:{accent};color:{accentText(accent)}">
            Download all{source?.bytes ? ` · ${formatBytes(source.bytes)}` : ""}
          </div>
        {/if}
        {#if expiryShort}<div class="text-[11px] text-faint">{expiryShort}</div>{/if}
      </div>
    </div>
    {#if editing && link}
      <p class="mt-2 text-xs text-faint">{kindNoun(link.kind)} link · created {new Date(link.created_at).toLocaleDateString()}</p>
    {/if}
  </aside>
</div>

{#if kind === "download"}
  <FrameioPicker
    bind:open={sourceOpen}
    mode="source"
    title="Send files"
    start={source?.path?.length ? { account_id: source.source.account_id, folder_id: source.source.type === "folder" ? source.source.folder_id : source.path[source.path.length - 1].id, path: source.path } : (recents[0] ?? null)}
    {recents}
    onsource={onSource}
  />
{/if}
