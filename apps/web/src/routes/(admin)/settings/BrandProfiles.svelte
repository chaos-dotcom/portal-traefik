<script lang="ts">
  // Named brands a link can carry instead of the default one: page name, accent colour, logo.
  import { invalidateAll } from "$app/navigation";
  import { ConfirmButton, Icon, type BrandProfile } from "$lib/components";
  import { errorDetail, toast } from "$lib/toast.svelte";

  let { brands }: { brands: BrandProfile[] } = $props();

  let editing = $state<string | null>(null); // a profile id, or "new"
  let name = $state("");
  let displayName = $state("");
  let accent = $state("");
  let busy = $state(false);

  function open(b: BrandProfile | null) {
    editing = b ? b.id : "new";
    name = b?.name ?? "";
    displayName = b?.display_name ?? "";
    accent = b?.accent_color ?? "";
  }

  async function save() {
    busy = true;
    try {
      const isNew = editing === "new";
      const res = await fetch(isNew ? "/api/brand-profiles" : `/api/brand-profiles/${editing}`, {
        method: isNew ? "POST" : "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name, display_name: displayName, accent_color: accent }),
      });
      if (!res.ok) return toast(await errorDetail(res, "Could not save the brand"), { kind: "error" });
      const saved = (await res.json()) as BrandProfile;
      toast(isNew ? `Added “${saved.name}”. Add a logo next.` : "Saved");
      editing = saved.id;
      await invalidateAll();
    } finally {
      busy = false;
    }
  }

  async function uploadLogo(id: string, e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    const fd = new FormData();
    fd.append("file", file);
    const res = await fetch(`/api/brand-profiles/${id}/logo`, { method: "POST", body: fd });
    if (!res.ok) return toast(await errorDetail(res, "Upload failed"), { kind: "error" });
    toast("Logo updated");
    await invalidateAll();
  }

  async function removeLogo(id: string) {
    await fetch(`/api/brand-profiles/${id}/logo`, { method: "DELETE" });
    await invalidateAll();
  }

  async function remove(b: BrandProfile) {
    const res = await fetch(`/api/brand-profiles/${b.id}`, { method: "DELETE" });
    if (!res.ok) return toast(await errorDetail(res, "Could not delete"), { kind: "error" });
    toast(`Deleted “${b.name}”${b.links ? `. Its ${b.links} link${b.links === 1 ? "" : "s"} use the default brand now.` : ""}`);
    editing = null;
    await invalidateAll();
  }

  const input = "h-8 w-full rounded-md border border-border bg-bg px-2.5 text-[13px] text-text outline-none placeholder:text-faint focus:border-accent";
</script>

<div>
  {#each brands as b (b.id)}
    <div class="border-t border-border px-5 py-3 first:border-t-0">
      <div class="flex items-center gap-3">
        <span class="grid h-9 w-16 shrink-0 place-items-center overflow-hidden rounded-md border border-border bg-bg">
          {#if b.logo_url}<img src={b.logo_url} alt="" class="max-h-7 max-w-14 object-contain" />{:else}<span class="h-3.5 w-3.5 rounded-full" style="background:{b.accent_color || 'var(--color-accent)'}"></span>{/if}
        </span>
        <div class="min-w-0 flex-1">
          <div class="truncate font-medium text-text">{b.name}</div>
          <div class="truncate text-xs text-faint">{b.display_name ?? "Uses the default name"} · {b.links} link{b.links === 1 ? "" : "s"}</div>
        </div>
        <button type="button" class="h-7 rounded-md px-2.5 text-xs font-medium text-muted hover:bg-surface-3 hover:text-text" onclick={() => (editing === b.id ? (editing = null) : open(b))}>{editing === b.id ? "Close" : "Edit"}</button>
        <ConfirmButton label="Delete brand" confirmLabel="Delete" icon="trash" onconfirm={() => remove(b)} />
      </div>
      {#if editing === b.id}
        {@render form(b)}
      {/if}
    </div>
  {:else}
    {#if editing !== "new"}
      <p class="px-5 py-4 text-muted">No brands yet. Add one to deliver under a client's or production's name and logo.</p>
    {/if}
  {/each}
  {#if editing === "new"}
    <div class="border-t border-border px-5 py-3 first:border-t-0">{@render form(null)}</div>
  {:else}
    <div class="border-t border-border px-5 py-3">
      <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-3 font-medium text-text hover:bg-surface-3" onclick={() => open(null)}><Icon name="plus" size={15} />New brand</button>
    </div>
  {/if}
</div>

{#snippet form(b: BrandProfile | null)}
  <div class="mt-3 grid gap-3 sm:grid-cols-2">
    <label class="grid gap-1 text-xs text-faint">Name (only you see it)<input class={input} bind:value={name} placeholder="e.g. Northlight Films" /></label>
    <label class="grid gap-1 text-xs text-faint">Name on pages and emails<input class={input} bind:value={displayName} placeholder="Defaults to your own" /></label>
    <label class="grid gap-1 text-xs text-faint">Accent colour
      <span class="flex items-center gap-2">
        <input type="color" class="h-8 w-10 cursor-pointer rounded border border-border bg-bg" value={accent || "#f59e0b"} oninput={(e) => (accent = (e.target as HTMLInputElement).value)} aria-label="Pick accent colour" />
        <input class={input} bind:value={accent} placeholder="#f59e0b" spellcheck="false" />
      </span>
    </label>
    {#if b}
      <div class="grid gap-1 text-xs text-faint">Logo (PNG)
        <span class="flex items-center gap-2">
          <label class="inline-flex h-8 cursor-pointer items-center rounded-md border border-border px-3 text-[13px] font-medium text-text hover:bg-surface-3">
            {b.has_logo ? "Replace logo" : "Upload logo"}
            <input type="file" accept="image/png,.png" class="hidden" onchange={(e) => uploadLogo(b.id, e)} />
          </label>
          {#if b.has_logo}<ConfirmButton label="Remove" confirmLabel="Remove logo" onconfirm={() => removeLogo(b.id)} />{/if}
        </span>
      </div>
    {/if}
  </div>
  <div class="mt-3 flex gap-2">
    <button type="button" class="inline-flex h-8 items-center rounded-md bg-accent px-4 font-medium text-on-accent hover:bg-accent-hover disabled:opacity-50" disabled={busy || !name.trim()} onclick={save}>{busy ? "Saving…" : b ? "Save" : "Add brand"}</button>
    <button type="button" class="h-8 rounded-md px-3 font-medium text-muted hover:text-text" onclick={() => (editing = null)}>Cancel</button>
  </div>
{/snippet}
