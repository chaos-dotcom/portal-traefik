<script lang="ts">
  import { goto } from "$app/navigation";
  import { untrack } from "svelte";
  import { FolderField, Icon, Switch, TemplateTokenInput } from "$lib/components";
  import { coveringRules, crumbNames, type FolderRef } from "$lib/frameio";
  import { recentFolders } from "$lib/recents";
  import { errorDetail, toast } from "$lib/toast.svelte";
  import { SYNC_TOKENS } from "$lib/tokens";
  import type { SyncRule } from "../+page";

  let { data } = $props();
  const rules = $derived(data.rules as SyncRule[]);
  const recents = $derived(recentFolders(data.uploads));

  // Seeded once from the page data (a "Sync this folder" link); the form owns them after that.
  const initial = untrack(() => data.folder);
  let folder = $state<FolderRef | null>(initial);
  let name = $state(initial ? (crumbNames(initial).at(-1) ?? "") : "");
  let nameTouched = $state(false);
  let destinationPath = $state("");
  let pathTouched = $state(false);
  let recursive = $state(true);
  let conflictPolicy = $state("rename_suffix");
  let pathTemplate = $state("");
  let enabled = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);

  // Suggest a NAS path next to the ones existing rules use: <their parent>/<folder name>.
  const nasParent = $derived.by(() => {
    const p = rules[0]?.destination_path?.replace(/\/+$/, "");
    return p && p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "";
  });
  $effect(() => {
    if (!pathTouched && folder && nasParent) destinationPath = `${nasParent}/${crumbNames(folder).at(-1)}`;
  });

  const already = $derived(coveringRules(folder, rules.map((r) => ({ ...r, source: r.source }))));

  function onFolder(f: FolderRef) {
    if (!nameTouched) name = crumbNames(f).at(-1) ?? "";
  }

  async function create() {
    error = null;
    if (!folder) {
      error = "Choose the Frame.io folder to watch.";
      return;
    }
    if (!destinationPath.trim()) {
      error = "Enter where on the NAS the files should go.";
      return;
    }
    saving = true;
    try {
      const res = await fetch("/api/sync-rules", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: name.trim() || crumbNames(folder).at(-1),
          source: {
            type: "frameio",
            account_id: folder.account_id,
            workspace_id: folder.workspace_id,
            project_id: folder.project_id,
            folder_id: folder.folder_id,
            recursive,
            // Names + path ride along so the backend skips the tightly rate-limited lookups.
            folder_name: folder.folder_name,
            project_name: folder.project_name,
            path: folder.path,
          },
          destination_path: destinationPath.trim(),
          conflict_policy: conflictPolicy,
          path_template: pathTemplate.trim() || null,
          enabled,
        }),
      });
      if (!res.ok) {
        error = await errorDetail(res, "Could not create the rule");
        return;
      }
      toast(`Watching ${crumbNames(folder).at(-1)}. Files already there follow on the next scheduled check.`);
      await goto("/sync");
    } finally {
      saving = false;
    }
  }

  const input = "h-8 w-full rounded-md border border-border bg-bg px-2.5 text-[13px] text-text outline-none placeholder:text-faint hover:border-border-strong focus:border-accent focus:ring-3 focus:ring-accent-soft";
  const line = "grid grid-cols-1 gap-2 border-t border-border py-3.5 sm:grid-cols-[120px_minmax(0,1fr)] sm:gap-4";
  const label = "pt-2 text-xs font-medium text-faint";
</script>

<div class="mx-auto max-w-3xl text-[13px]">
  <div class="rounded-xl border border-border bg-surface">
    <div class="flex items-center gap-3 border-b border-border px-5 py-3.5 sm:px-7">
      <h1 class="flex-1 text-base font-semibold text-text">New sync rule</h1>
      <a href="/sync" class="grid h-8 w-8 place-items-center rounded-md text-faint hover:bg-surface-3 hover:text-text" aria-label="Close"><Icon name="x" /></a>
    </div>
    <div class="px-5 pt-3 pb-2 sm:px-7">
      {#if error}<p class="mb-3 rounded-md bg-danger/10 px-3 py-2 text-danger">{error}</p>{/if}

      <div class="{line} border-t-0">
        <div class={label}>Watch</div>
        <div class="grid min-w-0 gap-2">
          <FolderField bind:value={folder} {recents} title="Watch this Frame.io folder" chooseVerb="Watch" onchange={onFolder} />
          <label class="flex items-center gap-2 text-muted"><input type="checkbox" bind:checked={recursive} /> Include subfolders</label>
          {#if already.length}
            <p class="flex items-start gap-2 text-xs text-warning">
              <Icon name="warning" size={14} />
              <span>“{already[0].name}” already copies this folder{already[0].exact ? "" : " (through a parent folder)"} to <span class="font-mono">{already[0].destination_path}</span>. A second rule copies its files twice.</span>
            </p>
          {/if}
        </div>
      </div>

      <div class={line}>
        <div class={label}><label for="sr-path">Copy to</label></div>
        <div class="grid gap-1.5">
          <input id="sr-path" class="{input} font-mono text-xs" bind:value={destinationPath} oninput={() => (pathTouched = true)} placeholder="/volume1/Clients/Northlight" autocomplete="off" spellcheck="false" />
          <div class="text-xs text-faint">A folder on the NAS, as the Portal container sees it. Subfolders from Frame.io are kept.</div>
        </div>
      </div>

      <div class={line}>
        <div class={label}><label for="sr-name">Name</label></div>
        <input id="sr-name" class={input} bind:value={name} oninput={() => (nameTouched = true)} placeholder="Defaults to the folder name" autocomplete="off" />
      </div>

      <div class={line}>
        <div class={label}><label for="sr-conflict">Same name</label></div>
        <select id="sr-conflict" class="{input} max-w-72" bind:value={conflictPolicy}>
          <option value="rename_suffix">Keep both (add a number)</option>
          <option value="skip">Keep the file already there</option>
          <option value="overwrite">Replace it</option>
        </select>
      </div>

      <div class={line}>
        <div class={label}>Organize</div>
        <div class="min-w-0">
          <TemplateTokenInput bind:value={pathTemplate} tokens={SYNC_TOKENS} placeholder={"{project}/{date}/{filename}"} />
          <p class="mt-1 text-xs text-faint">Leave empty to keep Frame.io's folder structure as it is.</p>
        </div>
      </div>

      <div class={line}>
        <div class={label}>Start</div>
        <div class="flex items-center gap-3">
          <div class="min-w-0 flex-1">
            <b class="block font-medium text-text">Watch now</b>
            <small class="block text-xs text-faint">Creates the Frame.io webhook so new files are copied as soon as they arrive</small>
          </div>
          <Switch bind:checked={enabled} label="Watch now" />
        </div>
      </div>
    </div>
    <div class="flex items-center justify-end gap-2 border-t border-border px-5 py-3.5 sm:px-7">
      <a href="/sync" class="inline-flex h-8 items-center rounded-md px-3 font-medium text-muted hover:bg-surface-3 hover:text-text">Cancel</a>
      <button type="button" class="inline-flex h-8 items-center rounded-md bg-accent px-4 font-medium text-on-accent hover:bg-accent-hover disabled:opacity-50" disabled={saving} onclick={create}>
        {saving ? "Creating…" : "Create rule"}
      </button>
    </div>
  </div>
</div>
