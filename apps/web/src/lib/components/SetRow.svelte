<script lang="ts">
  // One row of a settings card: what it is (and a line of help) on the left, the control on the
  // right. Stacks on narrow screens.
  import type { Snippet } from "svelte";

  let {
    label,
    help = undefined,
    for: forId = undefined,
    children,
  }: { label: string; help?: string; for?: string; children: Snippet } = $props();
</script>

<div class="grid gap-x-6 gap-y-2 border-t border-border px-5 py-4 first:border-t-0 sm:grid-cols-[minmax(0,200px)_minmax(0,1fr)]">
  <div class="min-w-0">
    {#if forId}<label for={forId} class="block font-medium text-text">{label}</label>{:else}<div class="font-medium text-text">{label}</div>{/if}
    {#if help}<p class="mt-0.5 text-xs text-faint">{help}</p>{/if}
  </div>
  <div class="min-w-0 self-center">{@render children()}</div>
</div>
