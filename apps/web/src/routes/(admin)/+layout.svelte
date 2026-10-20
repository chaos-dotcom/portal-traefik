<script lang="ts">
  import { onNavigate } from "$app/navigation";
  import { page } from "$app/state";
  import { onMount } from "svelte";
  import { Icon, Toaster, type IconName } from "$lib/components";
  import { onLiveChange, startLive, stopLive } from "$lib/live.svelte";

  let { children } = $props();

  // View transitions API — crossfade between pages
  onNavigate((navigation) => {
    if (!document.startViewTransition) return;
    return new Promise((resolve) => {
      document.startViewTransition(async () => {
        resolve();
        await navigation.complete;
      });
    });
  });

  // What the nav badges show; refreshed on every live change.
  type Nav = { links_active: number; uploads_live: number; sync_active: number; sync_failed: number };
  let nav = $state<Nav | null>(null);
  async function loadNav() {
    try {
      const res = await fetch("/api/nav");
      if (res.ok) nav = await res.json();
    } catch {
      /* offline for a moment; the next change retries */
    }
  }
  onMount(() => {
    startLive();
    void loadNav();
    const off = onLiveChange(() => void loadNav());
    return () => {
      off();
      stopLive();
    };
  });

  type Section = { label: string; href: string; match: string[]; icon: IconName };
  const sections: Section[] = [
    { label: "Dashboard", href: "/", match: ["/activity", "/history"], icon: "home" },
    { label: "Files", href: "/explorer", match: ["/explorer"], icon: "folder" },
    { label: "Links", href: "/links", match: ["/links", "/upload-links", "/download-links", "/destinations"], icon: "link" },
    { label: "Sync", href: "/sync", match: ["/sync", "/sync-rules"], icon: "sync" },
    { label: "Settings", href: "/settings", match: ["/settings", "/connections"], icon: "sliders" },
  ];

  function active(s: Section): boolean {
    const p = page.url.pathname;
    if (s.href === "/") return p === "/" || s.match.some((m) => p.startsWith(m));
    return p === s.href || [s.href, ...s.match].some((m) => p === m || p.startsWith(m + "/"));
  }
</script>

<div class="min-h-full">
  <main class="mx-auto max-w-7xl px-4 py-7 pb-28 sm:px-6 sm:pb-7 sm:pt-24">
    {@render children()}
  </main>

  <!-- Floating pill navigation — full-width at the bottom on mobile, centered at the top on desktop -->
  <nav
    class="fixed bottom-5 left-4 right-4 z-40 flex items-center rounded-full border border-border bg-surface/85 px-2 py-2 shadow-xl backdrop-blur-md sm:bottom-auto sm:left-1/2 sm:right-auto sm:top-5 sm:w-auto sm:-translate-x-1/2"
    aria-label="Main navigation"
  >
    {#each sections as s (s.label)}
      {@const on = active(s)}
      <a
        href={s.href}
        class="relative flex flex-1 items-center justify-center gap-2 rounded-full px-4 py-2.5 text-sm font-medium transition-colors sm:flex-none {on
          ? 'bg-surface-3 text-accent'
          : 'text-muted hover:bg-surface-2 hover:text-text'}"
        aria-current={on ? "page" : undefined}
      >
        <Icon name={s.icon} size={19} />
        <span class="hidden sm:inline">{s.label}</span>
        {#if s.href === "/" && nav?.uploads_live}
          <span class="live-dot absolute top-2 right-3 sm:static" title="{nav.uploads_live} upload{nav.uploads_live === 1 ? '' : 's'} arriving now"></span>
        {:else if s.href === "/links" && nav?.links_active}
          <span class="tnum hidden h-[18px] min-w-[18px] place-items-center rounded-full px-1.5 text-[11px] font-semibold sm:grid {on ? 'bg-accent text-on-accent' : 'bg-surface-3 text-body'}" title="{nav.links_active} active link{nav.links_active === 1 ? '' : 's'}">{nav.links_active}</span>
        {:else if s.href === "/sync" && nav?.sync_failed}
          <span class="tnum absolute top-1 right-2 grid h-[18px] min-w-[18px] place-items-center rounded-full bg-danger px-1.5 text-[11px] font-semibold text-bg sm:static" title="{nav.sync_failed} sync job{nav.sync_failed === 1 ? '' : 's'} failed">{nav.sync_failed}</span>
        {:else if s.href === "/sync" && nav?.sync_active}
          <span class="live-dot absolute top-2 right-3 sm:static" title="{nav.sync_active} file{nav.sync_active === 1 ? '' : 's'} copying or queued"></span>
        {/if}
      </a>
    {/each}
  </nav>
</div>

<Toaster />
