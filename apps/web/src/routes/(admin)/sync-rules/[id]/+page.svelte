<script lang="ts">
  import { goto, invalidateAll } from "$app/navigation";
  import { ConfirmButton, Icon, Switch, TemplateTokenInput } from "$lib/components";
  import { crumbNames } from "$lib/frameio";
  import { formatBytes } from "$lib/format";
  import { refreshOnChange } from "$lib/live.svelte";
  import { timeAgo, whenFull } from "$lib/links";
  import { errorDetail, toast } from "$lib/toast.svelte";
  import { SYNC_TOKENS } from "$lib/tokens";
  import type { SyncJob } from "./+page";

  let { data } = $props();
  const rule = $derived(data.rule);
  const jobs = $derived(data.jobs as SyncJob[]);
  const stats = $derived(data.stats.counts as Record<string, number>);

  $effect(() => refreshOnChange(20_000));

  let name = $state("");
  let destinationPath = $state("");
  let conflictPolicy = $state("rename_suffix");
  let pathTemplate = $state("");
  let saving = $state(false);
  let error = $state<string | null>(null);

  let seededId = "";
  $effect(() => {
    if (rule.id !== seededId) {
      seededId = rule.id;
      name = rule.name;
      destinationPath = rule.destination_path;
      conflictPolicy = rule.conflict_policy;
      pathTemplate = rule.path_template ?? "";
    }
  });

  async function save() {
    saving = true;
    error = null;
    try {
      const res = await fetch(`/api/sync-rules/${rule.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: name.trim(),
          destination_path: destinationPath.trim(),
          conflict_policy: conflictPolicy,
          path_template: pathTemplate.trim() || null,
        }),
      });
      if (!res.ok) {
        error = await errorDetail(res, "Could not save");
        return;
      }
      toast("Saved");
      await invalidateAll();
    } finally {
      saving = false;
    }
  }

  async function setEnabled(v: boolean) {
    const res = await fetch(`/api/sync-rules/${rule.id}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled: v }),
    });
    if (!res.ok) {
      toast(await errorDetail(res, "Could not update"), { kind: "error" });
      await invalidateAll();
      return;
    }
    toast(v ? "Watching again" : "Paused");
    await invalidateAll();
  }

  async function runNow() {
    const res = await fetch(`/api/sync-rules/${rule.id}/run`, { method: "POST" });
    if (!res.ok) return toast(await errorDetail(res, "Could not check now"), { kind: "error" });
    const created = (await res.json().catch(() => ({}))).created ?? 0;
    toast(!rule.enabled ? "This rule is paused. Resume it to sync." : created > 0 ? `Queued ${created} file${created === 1 ? "" : "s"}` : "Up to date. Nothing new to copy.", { kind: created > 0 ? "ok" : "info" });
    await invalidateAll();
  }

  async function jobAction(id: string, action: "retry" | "dismiss") {
    const res = await fetch(`/api/sync-jobs/${id}/${action}`, { method: "POST" });
    if (!res.ok) return toast(await errorDetail(res, `Could not ${action}`), { kind: "error" });
    toast(action === "retry" ? "Trying that file again" : "Dismissed");
    await invalidateAll();
  }

  async function remove() {
    const res = await fetch(`/api/sync-rules/${rule.id}`, { method: "DELETE" });
    if (!res.ok) return toast(await errorDetail(res, "Could not delete"), { kind: "error" });
    toast(`Deleted “${rule.name}”. Files already on the NAS stay there.`);
    await goto("/sync");
  }

  const jobLabel: Record<string, string> = { done: "Copied", pending: "Queued", running: "Copying", waiting: "Waiting on Frame.io", skipped: "Skipped", dead_letter: "Failed" };
  const jobTone: Record<string, string> = { done: "text-muted", pending: "text-body", running: "text-accent-hover", waiting: "text-info", skipped: "text-faint", dead_letter: "text-danger" };

  const input = "h-8 w-full rounded-md border border-border bg-bg px-2.5 text-[13px] text-text outline-none placeholder:text-faint hover:border-border-strong focus:border-accent focus:ring-3 focus:ring-accent-soft";
  const line = "grid grid-cols-1 gap-2 border-t border-border py-3.5 sm:grid-cols-[120px_minmax(0,1fr)] sm:gap-4";
  const label = "pt-2 text-xs font-medium text-faint";
</script>

<div class="mx-auto max-w-4xl space-y-6 text-[13px]">
  <div>
    <a href="/sync" class="text-muted hover:text-text">← Sync</a>
    <div class="mt-2 flex flex-wrap items-center gap-3">
      <h1 class="min-w-0 flex-1 truncate text-xl font-semibold tracking-tight">{rule.name}</h1>
      <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-surface-2 px-3 font-medium text-text hover:bg-surface-3" onclick={runNow}><Icon name="retry" size={15} />Check now</button>
      <label class="flex items-center gap-2 text-muted">
        <Switch checked={rule.enabled} label="Watching" onchange={setEnabled} />
        {rule.enabled ? "Watching" : "Paused"}
      </label>
    </div>
    <div class="mt-1.5 flex min-w-0 flex-wrap items-center gap-1.5 text-faint">
      <span>{crumbNames(rule.source as never).join(" / ")}{(rule.source.recursive ?? true) ? " and subfolders" : ""}</span>
      <Icon name="move" size={13} />
      <span class="font-mono text-xs text-muted">{rule.destination_path}</span>
      {#if rule.enabled && !rule.subscription_active}<span class="text-warning">· webhook missing, checking on a timer</span>{/if}
    </div>
  </div>

  <section class="overflow-hidden rounded-xl border border-border bg-surface">
    <div class="flex flex-wrap items-center gap-3 border-b border-border px-5 py-3">
      <h2 class="flex-1 text-sm font-semibold">Files</h2>
      <span class="tnum text-xs text-faint">{[["done", "copied"], ["running", "copying"], ["pending", "queued"], ["waiting", "waiting"], ["dead_letter", "failed"], ["skipped", "skipped"]].filter(([k]) => stats[k]).map(([k, l]) => `${stats[k]} ${l}`).join(" · ") || "Nothing yet"}</span>
    </div>
    {#each jobs as j (j.id)}
      <div class="border-b border-border px-5 py-2 last:border-b-0">
        <div class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 sm:grid-cols-[minmax(0,1fr)_80px_150px_120px]">
          <span class="truncate text-text" title={j.file_name ?? j.frameio_file_id}>{j.file_name ?? j.frameio_file_id}</span>
          <span class="tnum hidden text-right text-muted sm:block">{j.bytes != null ? formatBytes(j.bytes) : ""}</span>
          <span class="hidden truncate sm:block {jobTone[j.status] ?? 'text-muted'}">{jobLabel[j.status] ?? j.status}{j.retry_count > 0 ? ` · ${j.retry_count} retr${j.retry_count === 1 ? "y" : "ies"}` : ""}</span>
          <span class="flex items-center justify-end gap-3 text-xs text-faint">
            {#if j.status === "dead_letter"}
              <button type="button" class="font-medium text-text hover:underline" onclick={() => jobAction(j.id, "retry")}>Retry</button>
              <button type="button" class="hover:text-text" onclick={() => jobAction(j.id, "dismiss")}>Dismiss</button>
            {:else}
              <span title={whenFull(j.completed_at ?? j.created_at)}>{timeAgo(j.completed_at ?? j.started_at ?? j.created_at)}</span>
            {/if}
          </span>
        </div>
        {#if j.error && j.status !== "done"}<div class="mt-0.5 truncate text-xs {j.status === 'dead_letter' ? 'text-danger' : 'text-faint'}" title={j.error}>{j.error}</div>{/if}
      </div>
    {:else}
      <p class="px-5 py-8 text-center text-faint">No files yet. New files in the folder show up here.</p>
    {/each}
  </section>

  <section class="rounded-xl border border-border bg-surface">
    <div class="border-b border-border px-5 py-3"><h2 class="text-sm font-semibold">Settings</h2></div>
    <div class="px-5 pb-1">
      {#if error}<p class="mt-3 rounded-md bg-danger/10 px-3 py-2 text-danger">{error}</p>{/if}
      <div class="{line} border-t-0">
        <div class={label}><label for="r-name">Name</label></div>
        <input id="r-name" class={input} bind:value={name} />
      </div>
      <div class={line}>
        <div class={label}><label for="r-path">Copy to</label></div>
        <input id="r-path" class="{input} font-mono text-xs" bind:value={destinationPath} spellcheck="false" />
      </div>
      <div class={line}>
        <div class={label}><label for="r-conflict">Same name</label></div>
        <select id="r-conflict" class="{input} max-w-72" bind:value={conflictPolicy}>
          <option value="rename_suffix">Keep both (add a number)</option>
          <option value="skip">Keep the file already there</option>
          <option value="overwrite">Replace it</option>
        </select>
      </div>
      <div class={line}>
        <div class={label}>Organize</div>
        <div class="min-w-0">
          <TemplateTokenInput bind:value={pathTemplate} tokens={SYNC_TOKENS} placeholder={"{project}/{date}/{filename}"} />
          <p class="mt-1 text-xs text-faint">Leave empty to keep Frame.io's folder structure. To watch a different folder, create a new rule.</p>
        </div>
      </div>
    </div>
    <div class="flex items-center gap-2 border-t border-border px-5 py-3">
      <ConfirmButton label="Delete rule" confirmLabel="Delete rule" onconfirm={remove} />
      <span class="flex-1"></span>
      <button type="button" class="inline-flex h-8 items-center rounded-md bg-accent px-4 font-medium text-on-accent hover:bg-accent-hover disabled:opacity-50" disabled={saving} onclick={save}>{saving ? "Saving…" : "Save changes"}</button>
    </div>
  </section>
</div>
