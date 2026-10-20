<script lang="ts">
  import { dismiss, toasts } from "$lib/toast.svelte";

  const dot: Record<string, string> = { ok: "bg-success", info: "bg-info", error: "bg-danger" };
</script>

<div
  class="pointer-events-none fixed right-4 bottom-24 z-[60] flex w-[min(380px,calc(100vw-2rem))] flex-col gap-2 sm:bottom-5"
  aria-live="polite"
>
  {#each toasts as t (t.id)}
    <div
      class="toast pointer-events-auto flex items-center gap-2.5 rounded-lg border border-border bg-surface-2 py-2.5 pr-2 pl-3.5 text-[13px] text-text shadow-xl"
      role="status"
    >
      <span class="h-[7px] w-[7px] shrink-0 rounded-full {dot[t.kind]}"></span>
      <span class="min-w-0 flex-1 break-words">{t.message}</span>
      {#if t.action}
        <button
          type="button"
          class="rounded px-2 py-1 font-semibold text-accent-hover hover:bg-surface-3"
          onclick={async () => {
            dismiss(t.id);
            await t.action?.run();
          }}>{t.action.label}</button
        >
      {/if}
    </div>
  {/each}
</div>

<style>
  @media (prefers-reduced-motion: no-preference) {
    .toast {
      animation: rise 0.16s ease;
    }
    @keyframes rise {
      from {
        transform: translateY(6px);
        opacity: 0;
      }
    }
  }
</style>
