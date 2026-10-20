<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { ConfirmButton, Icon } from "$lib/components";
  import { crumbNames } from "$lib/frameio";
  import { formatBytes } from "$lib/format";
  import { refreshOnChange } from "$lib/live.svelte";
  import { timeAgo, whenFull } from "$lib/links";
  import { errorDetail, toast } from "$lib/toast.svelte";
  import type { SyncRule } from "./+page";
  import type { SyncJob } from "../sync-rules/[id]/+page";

  let { data } = $props();
  const rules = $derived(data.rules as SyncRule[]);

  $effect(() => refreshOnChange(20_000));

  const base = (p: string | null | undefined) => (p ?? "").split("/").pop() || p || "";

  type Tone = "live" | "ok" | "muted" | "warn" | "bad" | "wait";
  function ruleStatus(r: SyncRule): { tone: Tone; text: string; title?: string } {
    if (!r.enabled) return { tone: "muted", text: "Paused" };
    const a = r.activity;
    const c = a?.counts ?? {};
    if (c.running) return { tone: "live", text: a?.running_file ? `Copying ${base(a.running_file)}` : "Copying", title: a?.running_file ?? undefined };
    if (c.pending) return { tone: "live", text: `${c.pending} file${c.pending === 1 ? "" : "s"} queued` };
    if (c.dead_letter) return { tone: "bad", text: `${c.dead_letter} failed`, title: a?.failed_file ?? undefined };
    if (c.waiting)
      return c.waiting === 1 && a?.waiting_file
        ? { tone: "wait", text: `Waiting · ${base(a.waiting_file)} not ready on Frame.io yet`, title: a.waiting_file }
        : { tone: "wait", text: `Waiting for ${c.waiting} files to finish on Frame.io` };
    if (!r.subscription_active) return { tone: "warn", text: "Watching, checks on a timer", title: "The Frame.io webhook is missing; new files are picked up by the periodic check instead of right away." };
    return { tone: "ok", text: "Watching" };
  }
  const tone: Record<Tone, string> = {
    live: "text-text",
    ok: "text-body",
    muted: "text-muted",
    warn: "text-warning",
    bad: "text-danger",
    wait: "text-info",
  };
  const dot: Record<Exclude<Tone, "live">, string> = { ok: "bg-success", muted: "bg-faint", warn: "bg-warning", bad: "bg-danger", wait: "bg-info" };

  let busy = $state<string | null>(null);

  async function run(r: SyncRule) {
    busy = r.id;
    try {
      const res = await fetch(`/api/sync-rules/${r.id}/run`, { method: "POST" });
      if (!res.ok) return toast(await errorDetail(res, "Could not check now"), { kind: "error" });
      const created = (await res.json().catch(() => ({}))).created ?? 0;
      toast(
        !r.enabled
          ? `“${r.name}” is paused. Resume it to sync.`
          : created > 0
            ? `Queued ${created} file${created === 1 ? "" : "s"} from “${r.name}”`
            : `“${r.name}” is up to date`,
        { kind: created > 0 ? "ok" : "info" },
      );
      await invalidateAll();
    } finally {
      busy = null;
    }
  }

  async function setEnabled(r: SyncRule, enabled: boolean) {
    busy = r.id;
    try {
      const res = await fetch(`/api/sync-rules/${r.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      if (!res.ok) return toast(await errorDetail(res, "Could not update"), { kind: "error" });
      toast(enabled ? `“${r.name}” is watching again` : `Paused “${r.name}”`);
      await invalidateAll();
    } finally {
      busy = null;
    }
  }

  async function remove(r: SyncRule) {
    const res = await fetch(`/api/sync-rules/${r.id}`, { method: "DELETE" });
    if (!res.ok) return toast(await errorDetail(res, "Could not delete"), { kind: "error" });
    toast(`Deleted “${r.name}”. Files already on the NAS stay there.`);
    await invalidateAll();
  }

  // ── expanded rows: recent files ─────────────────────────────────────────
  let open = $state(new Set<string>());
  let jobs = $state<Record<string, SyncJob[] | "error">>({});

  async function loadJobs(id: string) {
    const res = await fetch(`/api/sync-rules/${id}/jobs?limit=15`);
    jobs[id] = res.ok ? await res.json() : "error";
  }
  function toggle(r: SyncRule) {
    const next = new Set(open);
    if (next.has(r.id)) next.delete(r.id);
    else {
      next.add(r.id);
      void loadJobs(r.id);
    }
    open = next;
  }
  let lastRules: SyncRule[] | null = null;
  $effect(() => {
    if (rules !== lastRules) {
      lastRules = rules;
      for (const id of open) void loadJobs(id);
    }
  });

  async function jobAction(ruleId: string, id: string, action: "retry" | "dismiss") {
    const res = await fetch(`/api/sync-jobs/${id}/${action}`, { method: "POST" });
    if (!res.ok) return toast(await errorDetail(res, `Could not ${action}`), { kind: "error" });
    toast(action === "retry" ? "Trying that file again" : "Dismissed");
    await loadJobs(ruleId);
    await invalidateAll();
  }

  const jobLabel: Record<string, string> = {
    done: "Copied",
    pending: "Queued",
    running: "Copying",
    waiting: "Waiting on Frame.io",
    skipped: "Skipped",
    dead_letter: "Failed",
  };
  const jobTone: Record<string, string> = {
    done: "text-muted",
    pending: "text-body",
    running: "text-accent-hover",
    waiting: "text-info",
    skipped: "text-faint",
    dead_letter: "text-danger",
  };

  const grid =
    "grid items-center gap-x-3.5 grid-cols-[minmax(0,1fr)_auto] md:grid-cols-[minmax(0,1fr)_minmax(150px,0.9fr)_auto] lg:grid-cols-[minmax(0,1.5fr)_minmax(170px,1fr)_100px_110px_auto]";
</script>

<div class="space-y-4 text-[13px]">
  <div class="flex items-center gap-3">
    <h1 class="text-xl font-semibold tracking-tight">Sync</h1>
    <span class="flex-1"></span>
    <a href="/sync-rules/new" class="inline-flex h-8 items-center gap-1.5 rounded-md bg-accent px-3 font-medium text-on-accent hover:bg-accent-hover"><Icon name="plus" size={15} />New sync rule</a>
  </div>
  <p class="max-w-3xl text-muted">Each rule copies new files from a Frame.io folder to the NAS, once. Nothing is ever deleted on either side, and files you move or rename on the NAS stay where you put them.</p>

  {#if data.failed}
    <p class="rounded-md bg-danger/10 px-3 py-2 text-danger">Could not load sync rules. Refresh to try again.</p>
  {/if}

  <div>
    <div class="{grid} h-8 border-b border-border px-3 text-[11.5px] font-medium text-faint">
      <span>Rule</span>
      <span class="hidden md:block">Status</span>
      <span class="hidden text-right lg:block">Files</span>
      <span class="hidden lg:block">Last copy</span>
      <span></span>
    </div>

    {#each rules as r (r.id)}
      {@const st = ruleStatus(r)}
      {@const c = r.activity?.counts ?? {}}
      {@const isOpen = open.has(r.id)}
      <div class="border-b border-border {isOpen ? 'bg-surface/60' : ''} {r.enabled ? '' : 'opacity-75'}">
        <div
          class="{grid} min-h-[60px] cursor-pointer px-3 py-2 hover:bg-white/[0.02]"
          role="button"
          tabindex="0"
          aria-expanded={isOpen}
          onclick={() => toggle(r)}
          onkeydown={(e) => {
            if ((e.key === "Enter" || e.key === " ") && e.target === e.currentTarget) {
              e.preventDefault();
              toggle(r);
            }
          }}
        >
          <div class="flex min-w-0 items-center gap-3">
            <div class="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-md border border-border bg-surface-2 text-body"><Icon name="sync" /></div>
            <div class="min-w-0">
              <div class="truncate font-medium text-text">{r.name}</div>
              <div class="flex min-w-0 items-center gap-1.5 text-xs text-faint">
                <span class="truncate">{crumbNames(r.source as never).join(" / ")}{(r.source.recursive ?? true) ? "" : " (this folder only)"}</span>
                <span class="shrink-0"><Icon name="move" size={13} /></span>
                <span class="truncate font-mono text-[11px] text-muted">{r.destination_path}</span>
              </div>
              <div class="mt-1 truncate text-xs md:hidden {tone[st.tone]}">{st.text}</div>
            </div>
          </div>

          <div class="hidden min-w-0 md:block">
            <span class="inline-flex max-w-full items-center gap-2 text-[12.5px] {tone[st.tone]}" title={st.title}>
              {#if st.tone === "live"}<span class="live-dot"></span>{:else}<span class="h-[7px] w-[7px] shrink-0 rounded-full {dot[st.tone]}"></span>{/if}
              <span class="truncate">{st.text}</span>
            </span>
          </div>

          <div class="tnum hidden text-right lg:block">
            <span class="text-body">{c.done ?? 0}</span>
            {#if r.activity?.bytes_done}<small class="block text-xs text-faint">{formatBytes(r.activity.bytes_done)}</small>{/if}
          </div>

          <div class="hidden min-w-0 lg:block">
            {#if r.activity?.last_done_at}
              <span class="text-body" title={whenFull(r.activity.last_done_at)}>{timeAgo(r.activity.last_done_at)}</span>
              <small class="block truncate text-xs text-faint" title={r.activity.last_done_file ?? ""}>{base(r.activity.last_done_file)}</small>
            {:else}
              <span class="text-faint">—</span>
            {/if}
          </div>

          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="flex items-center justify-end gap-0.5" onclick={(e) => e.stopPropagation()}>
            <button type="button" class="grid h-7 w-7 place-items-center rounded-md text-faint hover:bg-surface-3 hover:text-text disabled:opacity-40" title="Check for new files now" aria-label="Check {r.name} now" disabled={busy === r.id} onclick={() => run(r)}><Icon name="retry" /></button>
            <button type="button" class="grid h-7 w-7 place-items-center rounded-md text-faint hover:bg-surface-3 hover:text-text disabled:opacity-40" title={r.enabled ? "Pause" : "Resume"} aria-label={r.enabled ? `Pause ${r.name}` : `Resume ${r.name}`} disabled={busy === r.id} onclick={() => setEnabled(r, !r.enabled)}>
              <Icon name={r.enabled ? "pause" : "play"} />
            </button>
            <a href="/sync-rules/{r.id}" class="grid h-7 w-7 place-items-center rounded-md text-faint hover:bg-surface-3 hover:text-text" title="Edit" aria-label="Edit {r.name}"><Icon name="pencil" /></a>
            <ConfirmButton label="Delete rule" confirmLabel="Delete" icon="trash" onconfirm={() => remove(r)} />
          </div>
        </div>

        {#if isOpen}
          {@const list = jobs[r.id]}
          <div class="px-3 pb-4 md:pl-[54px]">
            <div class="overflow-hidden rounded-lg border border-border bg-surface">
              <div class="flex items-center gap-3 border-b border-border px-3 py-2 text-[11.5px] font-medium text-faint">
                <span class="flex-1">Recent files</span>
                <span class="tnum">{[["done", "copied"], ["waiting", "waiting"], ["dead_letter", "failed"], ["skipped", "skipped"]].filter(([k]) => c[k as keyof typeof c]).map(([k, l]) => `${c[k as keyof typeof c]} ${l}`).join(" · ")}</span>
                <a href="/sync-rules/{r.id}" class="text-muted hover:text-text">All files</a>
              </div>
              {#if !list}
                <div class="px-3 py-3 text-faint">Loading…</div>
              {:else if list === "error"}
                <div class="px-3 py-3 text-danger">Could not load files.</div>
              {:else}
                {#each list as j (j.id)}
                  <div class="border-b border-border px-3 py-1.5 text-xs last:border-b-0">
                    <div class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 sm:grid-cols-[minmax(0,1fr)_72px_140px_auto]">
                      <span class="truncate text-text" title={j.file_name ?? j.frameio_file_id}>{j.file_name ?? j.frameio_file_id}</span>
                      <span class="tnum hidden text-right text-muted sm:block">{j.bytes != null ? formatBytes(j.bytes) : ""}</span>
                      <span class="hidden truncate sm:block {jobTone[j.status] ?? 'text-muted'}">{jobLabel[j.status] ?? j.status}{j.retry_count > 0 ? ` · ${j.retry_count} retr${j.retry_count === 1 ? "y" : "ies"}` : ""}</span>
                      <span class="flex items-center justify-end gap-2 text-faint">
                        {#if j.status === "dead_letter"}
                          <button type="button" class="font-medium text-text hover:underline" onclick={() => jobAction(r.id, j.id, "retry")}>Retry</button>
                          <button type="button" class="hover:text-text" onclick={() => jobAction(r.id, j.id, "dismiss")}>Dismiss</button>
                        {:else}
                          <span title={whenFull(j.completed_at ?? j.created_at)}>{timeAgo(j.completed_at ?? j.started_at ?? j.created_at)}</span>
                        {/if}
                      </span>
                    </div>
                    {#if j.error && j.status !== "done"}<div class="mt-0.5 truncate {j.status === 'dead_letter' ? 'text-danger' : 'text-faint'}" title={j.error}>{j.error}</div>{/if}
                  </div>
                {:else}
                  <div class="px-3 py-3 text-faint">No files yet. New files in the folder show up here.</div>
                {/each}
              {/if}
            </div>
          </div>
        {/if}
      </div>
    {:else}
      {#if !data.failed}
        <div class="py-16 text-center">
          <div class="mx-auto mb-3 grid h-11 w-11 place-items-center rounded-xl border border-border bg-surface text-faint"><Icon name="sync" size={22} /></div>
          <div class="text-sm font-medium text-text">No sync rules</div>
          <p class="mt-1 text-muted">A rule copies a Frame.io folder's new files to the NAS. <a href="/sync-rules/new" class="text-accent-hover hover:underline">Create one</a>.</p>
        </div>
      {/if}
    {/each}
  </div>
</div>
